use crate::Message;
use std::time::Duration;

/// A connected client that can be used to send messages to the server.
///
/// # Example
/// ```
/// # use futures::{Sink, SinkExt, Stream, StreamExt};
/// # use reqwest_websocket::RequestBuilderExt;
/// # use std::marker::PhantomData;
/// # use std::{
/// #     pin::{pin, Pin},
/// #     task::{Context, Poll},
/// # };
/// # use tokio_websocket_client::{connect, CloseCode, Connector, Handler, Message, RetryStrategy, StreamWrapper};
/// #
/// # struct DummyHandler;
/// #
/// # impl Handler for DummyHandler {
/// #     type Error = reqwest_websocket::Error;
/// #
/// #     async fn on_text(&mut self, text: &str) -> Result<(), Self::Error> {
/// #         log::info!("on_text received: {text}");
/// #         Ok(())
/// #     }
/// #
/// #     async fn on_binary(&mut self, buffer: &[u8]) -> Result<(), Self::Error> {
/// #         log::info!("on_binary received: {buffer:?}");
/// #         Ok(())
/// #     }
/// #
/// #     async fn on_connect(&mut self) {
/// #         log::info!("on_connect");
/// #     }
/// #
/// #     async fn on_connect_failure(&mut self) -> Result<RetryStrategy, Self::Error> {
/// #         log::info!("on_connect_failure");
/// #         Ok(RetryStrategy::Close)
/// #     }
/// #
/// #     async fn on_close(
/// #         &mut self,
/// #         code: CloseCode,
/// #         reason: &str,
/// #     ) -> Result<RetryStrategy, Self::Error> {
/// #         log::info!("on_close received: {code:?}: {reason}");
/// #         Ok(RetryStrategy::Close)
/// #     }
/// # }
/// #
/// # struct DummyMessage(reqwest_websocket::Message);
/// #
/// # impl From<reqwest_websocket::Message> for DummyMessage {
/// #     fn from(message: reqwest_websocket::Message) -> Self {
/// #         Self(message)
/// #     }
/// # }
/// #
/// # impl From<DummyMessage> for reqwest_websocket::Message {
/// #     fn from(message: DummyMessage) -> Self {
/// #         message.0
/// #     }
/// # }
/// #
/// # impl From<DummyMessage> for Message {
/// #     fn from(other: DummyMessage) -> Message {
/// #         match other {
/// #             DummyMessage(reqwest_websocket::Message::Text(data)) => Message::Text(data),
/// #             DummyMessage(reqwest_websocket::Message::Binary(data)) => Message::Binary(data),
/// #             DummyMessage(reqwest_websocket::Message::Ping(data)) => Message::Ping(data),
/// #             DummyMessage(reqwest_websocket::Message::Pong(data)) => Message::Pong(data),
/// #             DummyMessage(reqwest_websocket::Message::Close { code, reason }) => {
/// #                 Message::Close(CloseCode::from(u16::from(code)), reason)
/// #             }
/// #         }
/// #     }
/// # }
/// #
/// # impl From<Message> for DummyMessage {
/// #     fn from(msg: Message) -> Self {
/// #         match msg {
/// #             Message::Text(data) => Self(reqwest_websocket::Message::Text(data)),
/// #             Message::Binary(data) => Self(reqwest_websocket::Message::Binary(data)),
/// #             Message::Ping(data) => Self(reqwest_websocket::Message::Ping(data)),
/// #             Message::Pong(data) => Self(reqwest_websocket::Message::Pong(data)),
/// #             Message::Close(code, reason) => Self(reqwest_websocket::Message::Close {
/// #                 code: reqwest_websocket::CloseCode::from(u16::from(code)),
/// #                 reason,
/// #             }),
/// #         }
/// #     }
/// # }
/// #
/// # struct DummyConnector;
/// #
/// # impl Connector for DummyConnector {
/// #     type Item = DummyMessage;
/// #     type BackendStream = reqwest_websocket::WebSocket;
/// #     type BackendMessage = reqwest_websocket::Message;
/// #     type Error = reqwest_websocket::Error;
/// #
/// #     async fn connect() -> Result<StreamWrapper<'static, Self::BackendStream, Self::BackendMessage, Self::Item, Self::Error>, Self::Error> {
/// #         // Creates a GET request, upgrades and sends it.
/// #         let response = reqwest::Client::default()
/// #             .get("wss://echo.websocket.org/")
/// #             .upgrade() // Prepares the WebSocket upgrade.
/// #             .send()
/// #             .await?;
/// #
/// #         // Turns the response into a DummyStream.
/// #         response.into_websocket().await.map(StreamWrapper::from)
/// #     }
/// # }
/// # let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
/// # rt.block_on(async move {
/// let Some(client) = connect(DummyConnector, DummyHandler).await else {
///     log::info!("Failed to connect");
///     return;
/// };
///  
/// client.text("hello world").await.unwrap();
/// # });
/// ```
#[derive(Debug, Clone)]
pub struct Client {
    pub(crate) to_send: flume::Sender<Message>,
}

impl From<Client> for flume::Sender<Message> {
    fn from(client: Client) -> Self {
        client.to_send
    }
}

impl Client {
    /// Send a text message to the server.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendError) if all receivers have been dropped.
    pub async fn text(&self, message: impl Into<String>) -> Result<(), flume::SendError<Message>> {
        let message = message.into();
        log::debug!("Sending text: {}", &message);

        self.to_send.send_async(Message::Text(message)).await
    }

    /// Send a text message to the server.
    ///
    /// Allows to put a timeout on the queue sending the message.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendError) if all receivers have been dropped.
    pub async fn text_timeout(
        &self,
        message: impl Into<String>,
        timeout: Duration,
    ) -> Result<(), std::io::Error> {
        let message = message.into();
        log::debug!("Sending text: {}", &message);
        match tokio::time::timeout(timeout, self.to_send.send_async(Message::Text(message))).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Receiver channel has been dropped.",
            )),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Time out reached.",
            )),
        }
    }

    /// Send a binary message to the server.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendError) if all receivers have been dropped.
    pub async fn binary(
        &self,
        message: impl IntoIterator<Item = u8>,
    ) -> Result<(), flume::SendError<Message>> {
        let message = message.into_iter().collect();
        log::debug!("Sending text: {:?}", &message);

        self.to_send.send_async(Message::Binary(message)).await
    }

    /// Send a binary message to the server.
    ///
    /// Allows to put a timeout on the queue sending the message.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendError) if all receivers have been dropped.
    pub async fn binary_timeout(
        &self,
        message: impl IntoIterator<Item = u8>,
        timeout: Duration,
    ) -> Result<(), std::io::Error> {
        let message = message.into_iter().collect();
        log::debug!("Sending text: {:?}", &message);
        match tokio::time::timeout(timeout, self.to_send.send_async(Message::Binary(message))).await
        {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "Receiver channel has been dropped.",
            )),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Time out reached.",
            )),
        }
    }
}
