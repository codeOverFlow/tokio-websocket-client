#![deny(clippy::all, clippy::pedantic, clippy::nursery)]
#![doc=include_str!("../README.md")]

mod client;
mod connector;
mod handler;
mod message;

pub use crate::{
    client::Client,
    connector::Connector,
    handler::{Handler, RetryStrategy},
    message::{CloseCode, Message},
};

use futures::{Sink, SinkExt, StreamExt};

/// Connect to a websocket server using the provided connector.
///
/// This function will indefinitly try to connect to the server
/// unless the [`handler::on_connect_failure`](Handler::on_connect_failure) returns a [`RetryStrategy::Close`].
pub async fn connect<C, H>(mut connector: C, mut handler: H) -> Option<Client>
where
    C: Connector + 'static,
    <C::Stream as Sink<C::Item>>::Error: std::error::Error + Send,
    H: Handler + 'static,
{
    let (to_send_tx, to_send_rx) = flume::bounded(C::request_queue_size());

    let Ok(stream) = connect_stream(&mut connector, &mut handler).await else {
        return None;
    };

    tokio::spawn(async move {
        background_task(to_send_rx, stream, connector, handler).await;
    });

    Some(Client {
        to_send: to_send_tx,
    })
}

async fn reconnect<C, H>(
    stream: &mut C::Stream,
    connector: &mut C,
    handler: &mut H,
) -> Result<(), ()>
where
    C: Connector,
    <C::Stream as Sink<C::Item>>::Error: std::error::Error,
    H: Handler,
{
    if let Err(reason) = stream.close().await {
        log::error!("{reason}");
    }

    *stream = connect_stream(connector, handler).await?;

    Ok(())
}

async fn connect_stream<C, H>(connector: &mut C, handler: &mut H) -> Result<C::Stream, ()>
where
    C: Connector,
    <C::Stream as Sink<C::Item>>::Error: std::error::Error,
    H: Handler,
{
    loop {
        let stream = match C::connect().await {
            Ok(stream) => stream,
            Err(reason) => {
                log::error!("Failed to connect: {reason}");
                if matches!(handler.on_connect_failure().await, Ok(RetryStrategy::Close)) {
                    log::error!("Stop retrying to connect.");
                    break Err(());
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

#[allow(clippy::too_many_lines, clippy::redundant_pub_crate)]
async fn background_task<C, H>(
    to_send: flume::Receiver<Message>,
    mut stream: C::Stream,
    mut connector: C,
    mut handler: H,
) where
    C: Connector,
    <C::Stream as Sink<C::Item>>::Error: std::error::Error,
    H: Handler,
{
    let mut ping_interval = tokio::time::interval(C::ping_interval());
    let mut last_ping = 0u8;
    let mut ponged = true; // initially true to avoid mistaking it for a failed ping/pong

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                if ponged {
                    if let Err(reason) = stream.send(Message::Ping(vec![last_ping]).into()).await {
                        log::error!("{reason}");
                        if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                            break;
                        }
                    }
                    ponged = false;
                }
                else {
                    log::error!("Last ping has not been ponged");
                    if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                        break;
                    }
                }
            },
            res = to_send.recv_async() => {
                if let Ok(message) = res {
                    if let Err(reason) = stream.send(message.into()).await {
                        log::error!("{reason}");
                        if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                            break;
                        }
                    }
                } else {
                    log::info!("Closing the stream, all clients have been dropped");
                    break;
                }
            }
            Some(message) = stream.next() => {
                match message {
                    Ok(message) => match message.into() {
                        Message::Text(ref text) => {
                            if let Err(reason) = handler.on_text(text).await {
                                log::error!("{reason}");
                                if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Message::Binary(ref buf) => {
                            if let Err(reason) = handler.on_binary(buf).await {
                                log::error!("{reason}");
                                if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Message::Ping(data) => {
                            if let Err(reason) = stream.send(Message::Pong(data).into()).await {
                                log::error!("{reason}");
                                if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Message::Pong(buf) => {
                            if buf.len() != 1 {
                                log::error!("Pong data is invalid: {buf:?}");
                                if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                                    break;
                                }
                            }

                            if buf[0] == last_ping {
                                ponged = true;
                                last_ping = last_ping.wrapping_add(1);
                            } else {
                                log::error!("Pong data is invalid, expected {last_ping} got {:?}", buf[0]);
                                if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Message::Close(code, reason) => {
                            log::info!("Server closed with code {}: {reason}", u16::from(&code));
                            if let Err(reason) = handler.on_close(code.clone(), &reason).await {
                                log::error!("{reason}");
                            }

                            if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                                log::error!("Stop retrying to connect.");
                                break;
                            }
                        }
                    },
                    Err(reason) => {
                        log::error!("{reason}");
                        if reconnect(&mut stream, &mut connector, &mut handler).await.is_err() {
                            log::error!("Stop retrying to connect.");
                            break;
                        }
                    }
                }
            }
        }
    }

    if let Err(reason) = stream.close().await {
        log::error!("{reason}");
    }

    log::trace!("Background task complete");
}
