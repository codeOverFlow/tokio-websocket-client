#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![doc=include_str!("../README.md")]

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

#[doc(hidden)]
pub use crate::stream_wrapper::StreamWrapper;

use crate::command::Command;
use futures::{Sink, SinkExt, StreamExt};

#[derive(Debug, Clone, Eq, PartialEq)]
enum LoopControl {
    Continue,
    Break,
}

/// Connect to a websocket server using the provided connector.
///
/// This function will indefinitly try to connect to the server
/// unless the [`handler::on_connect_failure`](Handler::on_connect_failure) returns a [`RetryStrategy::Close`].
pub async fn connect<C, H>(mut connector: C, mut handler: H) -> Option<Client>
where
    C: Connector + 'static,
    H: Handler + 'static,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
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

    Some(Client {
        to_send: to_send_tx,
        command_tx,
        confirm_close_rx,
    })
}

async fn reconnect<C, H>(
    stream: &mut StreamWrapper<'static, C::BackendStream, C::BackendMessage, C::Item, C::Error>,
    connector: &mut C,
    handler: &mut H,
) -> Result<(), LoopControl>
where
    C: Connector,
    H: Handler,
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
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
    >>::Error: std::error::Error,
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
                tokio::time::sleep(delay).await;
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
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
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
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
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
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
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
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
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
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
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

                if buf[0] == *last_ping {
                    *ponged = true;
                    *last_ping = last_ping.wrapping_add(1);
                } else if !want_to_close {
                    log::error!(
                        "Pong data is invalid, expected {last_ping} got {:?}",
                        buf[0]
                    );
                    handle_reconnect(connector, handler, stream).await?;
                }
            }
            Message::Close(code, reason) => {
                if want_to_close {
                    return Err(LoopControl::Break);
                }

                log::info!("Server closed with code {}: {reason}", u16::from(&code));

                if let Err(reason) = stream
                    .send(C::Item::from(Message::Close(
                        code.clone(),
                        String::default(),
                    )))
                    .await
                {
                    log::error!("Failed to send back Close to stream: {reason}");
                }

                match handler.on_close(code, &reason).await {
                    RetryStrategy::Close => {
                        log::error!("Do not retry to connect.");
                        if let Err(reason) = stream.close().await {
                            log::error!("Failed to close the stream: {reason}");
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
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
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
                    "Client is explicitly closing the stream".to_string(),
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

#[allow(clippy::too_many_lines, clippy::redundant_pub_crate)]
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
    <C::BackendStream as Sink<C::BackendMessage>>::Error: std::error::Error + Send,
{
    let mut ping_interval = tokio::time::interval(C::ping_interval());
    let mut last_ping = 0u8;
    let mut ponged = true; // initially true to avoid mistaking it for a failed ping/pong
    let mut want_to_close = false;

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
