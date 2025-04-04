#![deny(
    warnings,
    absolute_paths_not_starting_with_crate,
    unused_unsafe,
    future_incompatible,
    keyword_idents,
    unused,
    let_underscore,
    refining_impl_trait,
    rust_2024_compatibility,
    clippy::all,
    clippy::pedantic,
    clippy::absolute_paths,
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason,
    clippy::arithmetic_side_effects,
    clippy::as_pointer_underscore,
    clippy::assertions_on_result_states,
    clippy::cfg_not_test,
    clippy::clone_on_ref_ptr,
    clippy::dbg_macro,
    clippy::decimal_literal_representation,
    clippy::deref_by_slicing,
    clippy::disallowed_script_idents,
    clippy::doc_include_without_cfg,
    clippy::empty_drop,
    clippy::empty_enum_variants_with_brackets,
    clippy::expect_used,
    clippy::field_scoped_visibility_modifiers,
    clippy::filetype_is_file,
    clippy::float_cmp_const,
    clippy::fn_to_numeric_cast_any,
    clippy::get_unwrap,
    clippy::if_then_some_else_none,
    clippy::indexing_slicing,
    clippy::infinite_loop,
    clippy::integer_division,
    clippy::integer_division_remainder_used,
    clippy::large_include_file,
    clippy::let_underscore_must_use,
    clippy::lossy_float_literal,
    clippy::map_err_ignore,
    clippy::map_with_unused_argument_over_ranges,
    clippy::mem_forget,
    clippy::min_ident_chars,
    clippy::missing_assert_message,
    clippy::missing_inline_in_public_items,
    clippy::mixed_read_write_in_expression,
    clippy::multiple_inherent_impl,
    clippy::multiple_unsafe_ops_per_block,
    clippy::undocumented_unsafe_blocks,
    clippy::needless_raw_strings,
    clippy::non_ascii_literal,
    clippy::non_zero_suggestions,
    clippy::panic,
    clippy::pattern_type_mismatch,
    clippy::precedence_bits,
    clippy::pub_without_shorthand,
    clippy::rc_buffer,
    clippy::rc_mutex,
    clippy::redundant_type_annotations,
    clippy::renamed_function_params,
    clippy::rest_pat_in_fully_bound_structs,
    clippy::return_and_then,
    clippy::same_name_method,
    clippy::self_named_module_files,
    clippy::semicolon_outside_block,
    clippy::unseparated_literal_suffix,
    clippy::shadow_unrelated,
    clippy::str_to_string,
    clippy::string_add,
    clippy::string_lit_chars_any,
    clippy::string_slice,
    clippy::string_to_string,
    clippy::suspicious_xor_used_as_pow,
    clippy::tests_outside_test_module,
    clippy::todo,
    clippy::try_err,
    clippy::unimplemented,
    clippy::unnecessary_safety_comment,
    clippy::unnecessary_self_imports,
    clippy::unneeded_field_pattern,
    clippy::unreachable,
    clippy::unused_result_ok,
    clippy::unused_trait_names,
    clippy::unwrap_used,
    clippy::verbose_file_reads
)]
#![cfg_attr(doc, doc=include_str!("../README.md"))]

mod client;
pub(crate) mod command;
mod connector;
mod handler;
mod message;
mod stream_wrapper;

pub use crate::{
    client::Client,
    connector::Connector,
    handler::{Handler, RetryStrategy},
    message::{CloseCode, Message},
};

use std::error::Error as StdError;
use tokio::time::{interval as TokioInterval, sleep as TokioSleep};

#[doc(hidden)]
pub use crate::stream_wrapper::StreamWrapper;

use crate::command::Command;
use futures::{Sink, SinkExt as _, StreamExt as _};

#[derive(Debug, Clone, Eq, PartialEq)]
enum LoopControl {
    Break,
    Continue,
}

/// Connect to a websocket server using the provided connector.
///
/// This function will indefinitly try to connect to the server
/// unless the [`handler::on_connect_failure`](Handler::on_connect_failure) returns a [`RetryStrategy::Close`].
#[inline]
pub async fn connect<C, H>(mut connector: C, mut handler: H) -> Option<Client>
where
    C: Connector + 'static,
    H: Handler + 'static,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    let (to_send_tx, to_send_rx) = flume::bounded(C::request_queue_size());
    let (command_tx, command_rx) = flume::bounded(1);
    let (confirm_close_tx, confirm_close_rx) = flume::bounded(1);

    let Ok(stream) = connect_stream(&mut connector, &mut handler).await else {
        return None;
    };

    tokio::spawn(async move {
        background_task(
            to_send_rx,
            command_rx,
            confirm_close_tx,
            stream,
            connector,
            handler,
        )
        .await;
    });

    Some(Client::new(to_send_tx, command_tx, confirm_close_rx))
}

async fn reconnect<C, H>(
    stream: &mut StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
    connector: &mut C,
    handler: &mut H,
) -> Result<(), LoopControl>
where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    if let Err(reason) = stream.close().await {
        log::error!("{reason}");
    }

    *stream = connect_stream(connector, handler).await?;

    Ok(())
}

async fn connect_stream<C, H>(
    connector: &mut C,
    handler: &mut H,
) -> Result<
    StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
    LoopControl,
>
where
    C: Connector,
    H: Handler,
    <StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error> as Sink<
        C::Item,
    >>::Error: StdError,
{
    loop {
        let stream = match C::connect().await {
            Ok(stream) => stream,
            Err(reason) => {
                log::error!("Failed to connect: {reason}");
                if handler.on_connect_failure().await == RetryStrategy::Close {
                    log::error!("Stop retrying to connect.");
                    break Err(LoopControl::Break);
                }
                let delay = connector.retry_delay().await;
                log::error!("Retrying in {}s", delay.as_secs());
                TokioSleep(delay).await;
                continue;
            }
        };

        log::info!("Connection Successfully established");
        handler.on_connect().await;

        break Ok(stream);
    }
}

async fn handle_disconnect<C, H>(
    connector: &mut C,
    handler: &mut H,
    stream: &mut StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
) -> Result<(), LoopControl>
where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    match handler.on_disconnect().await {
        RetryStrategy::Close => {
            log::error!("Do not retry to connect.");
            if let Err(reason) = stream.close().await {
                log::error!("Failed to close the stream: {reason}");
            }
            return Err(LoopControl::Break);
        }

        RetryStrategy::Retry => {
            if let Err(control) = reconnect(stream, connector, handler).await {
                log::error!("Do not retry to connect.");
                if let Err(reason) = stream.close().await {
                    log::error!("Failed to close the stream: {reason}");
                }
                // only Err on RetryStrategy::Close
                return Err(control);
            }
        }
    }

    Ok(())
}

async fn handle_ping_pong<C, H>(
    ponged: &mut bool,
    last_ping: u8,
    connector: &mut C,
    handler: &mut H,
    stream: &mut StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
) -> Result<(), LoopControl>
where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    if *ponged {
        if let Err(reason) = stream.send(Message::Ping(vec![last_ping]).into()).await {
            log::error!("Failed to send Ping: {reason}");
            handle_disconnect(connector, handler, stream).await?;
            return Err(LoopControl::Continue);
        }
        *ponged = false;
    } else {
        log::error!("Last ping has not been ponged");
        handle_disconnect(connector, handler, stream).await?;
    }

    Ok(())
}

async fn handle_message_to_send<C, H>(
    send_result: Result<Message, flume::RecvError>,
    connector: &mut C,
    handler: &mut H,
    stream: &mut StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
) -> Result<(), LoopControl>
where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    if let Ok(message) = send_result {
        if let Err(reason) = stream.send(message.into()).await {
            log::error!("Failed to send message on stream: {reason}");
            handle_disconnect(connector, handler, stream).await?;
        }
    } else {
        log::info!("Closing the stream, all clients have been dropped");
        return Err(LoopControl::Break);
    }

    Ok(())
}

async fn handle_reconnect<C, H>(
    connector: &mut C,
    handler: &mut H,
    stream: &mut StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
) -> Result<(), LoopControl>
where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    if reconnect(stream, connector, handler).await.is_err() {
        if let Err(reason) = stream.close().await {
            log::error!("Failed to close the stream: {reason}");
        }
        // only Err on RetryStrategy::Close
        return Err(LoopControl::Break);
    }

    Ok(())
}

async fn handle_message<C, H>(
    message: Result<C::Item, C::Error>,
    want_to_close: bool,
    ponged: &mut bool,
    last_ping: &mut u8,
    connector: &mut C,
    handler: &mut H,
    stream: &mut StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
) -> Result<(), LoopControl>
where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    match message {
        Ok(message) => match message.into() {
            Message::Text(ref text) => {
                handler.on_text(text).await;
            }
            Message::Binary(ref buf) => {
                handler.on_binary(buf).await;
            }
            Message::Ping(data) => {
                if let Err(reason) = stream.send(Message::Pong(data).into()).await {
                    if !want_to_close {
                        log::error!("Failed to send Pong to stream: {reason}");
                        handle_disconnect(connector, handler, stream).await?;
                    }
                }
            }
            Message::Pong(buf) => {
                if buf.len() != 1 {
                    log::error!("Pong data is invalid: {buf:?}");
                    handle_reconnect(connector, handler, stream).await?;
                    return Err(LoopControl::Continue);
                }

                if buf.first() == Some(last_ping) {
                    *ponged = true;
                    *last_ping = last_ping.wrapping_add(1);
                } else if !want_to_close {
                    log::error!(
                        "Pong data is invalid, expected {last_ping} got {:?}",
                        buf.first()
                    );
                    handle_reconnect(connector, handler, stream).await?;
                }
            }
            Message::Close(code, reason) => {
                if want_to_close {
                    return Err(LoopControl::Break);
                }

                log::info!("Server closed with code {}: {reason}", u16::from(&code));

                if let Err(error) = stream
                    .send(C::Item::from(Message::Close(
                        code.clone(),
                        String::default(),
                    )))
                    .await
                {
                    log::error!("Failed to send back Close to stream: {error}");
                }

                match handler.on_close(code, &reason).await {
                    RetryStrategy::Close => {
                        log::error!("Do not retry to connect.");
                        if let Err(error) = stream.close().await {
                            log::error!("Failed to close the stream: {error}");
                        }
                        return Err(LoopControl::Break);
                    }
                    RetryStrategy::Retry => {
                        handle_reconnect(connector, handler, stream).await?;
                    }
                }
            }
        },
        Err(reason) => {
            log::error!("Failed to read stream: {reason}");
            if !want_to_close {
                handle_reconnect(connector, handler, stream).await?;
            }
        }
    }

    Ok(())
}

async fn handle_command<C, H>(
    command: Result<Command, flume::RecvError>,
    want_to_close: &mut bool,
    connector: &mut C,
    handler: &mut H,
    stream: &mut StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
) -> Result<(), LoopControl>
where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    match command {
        Ok(Command::Reconnect) => {
            log::info!("Forcing reconnection");
            handle_reconnect(connector, handler, stream).await?;
        }
        Ok(Command::Close) => {
            log::info!("Client requested to close the connection");
            if let Err(reason) = stream
                .send(C::Item::from(Message::Close(
                    CloseCode::Normal,
                    "Client is explicitly closing the stream".to_owned(),
                )))
                .await
            {
                log::error!("Failed to send Close on the stream: {reason}");
                return Err(LoopControl::Break);
            }
            *want_to_close = true;
        }
        Err(_) => {
            log::info!("Closing the stream, all clients have been dropped");
            return Err(LoopControl::Break);
        }
    }

    Ok(())
}

async fn background_task<C, H>(
    to_send: flume::Receiver<Message>,
    command_rx: flume::Receiver<Command>,
    confirm_close_tx: flume::Sender<()>,
    mut stream: StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
    mut connector: C,
    mut handler: H,
) where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: StdError + Send,
{
    let mut ping_interval = TokioInterval(C::ping_interval());
    let mut last_ping = 0;
    let mut ponged = true; // initially true to avoid mistaking it for a failed ping/pong
    let mut want_to_close = false;

    #[expect(
        clippy::integer_division_remainder_used,
        reason = "use of tokio select!"
    )]
    #[expect(clippy::pattern_type_mismatch, reason = "use of tokio select!")]
    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                // do not ping if we are closing the connection.
                if !want_to_close && matches!(handle_ping_pong(&mut ponged, last_ping, &mut connector, &mut handler, &mut stream).await, Err(LoopControl::Break)) {
                    break;
                }
            },
            res = to_send.recv_async() => {
                if !want_to_close && matches!(handle_message_to_send(res, &mut connector, &mut handler, &mut stream).await, Err(LoopControl::Break)) {
                    break;
                }
            }
            res = command_rx.recv_async() => {
                if !want_to_close && matches!(handle_command(res, &mut want_to_close, &mut connector, &mut handler, &mut stream).await, Err(LoopControl::Break)) {
                    break;
                }
            }
            Some(message) = stream.next() => {
                if matches!(handle_message(message, want_to_close, &mut ponged, &mut last_ping, &mut connector, &mut handler, &mut stream).await, Err(LoopControl::Break)) {
                    break;
                }
            }
        }
    }

    if let Err(reason) = stream.close().await {
        log::error!("{reason}");
    }

    if let Err(reason) = confirm_close_tx.send_async(()).await {
        log::error!("Failed to send closing confirmation: {reason}");
    }

    log::trace!("Background task complete");
}
