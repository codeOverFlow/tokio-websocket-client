use crate::{Message, command::Command};
use flume::{RecvTimeoutError, SendTimeoutError};
use std::{
    io::{Error as IoError, ErrorKind as IoErrorKind, Result as IoResult},
    time::Duration,
};

/// A connected client that can be used to send messages to the server.
///
/// # Example
/// ```
/// # use reqwest_websocket::RequestBuilderExt;
/// # use tokio_websocket_client::{
/// #     CloseCode,
/// #     Connector,
/// #     Handler,
/// #     Message,
/// #     RetryStrategy,
/// #     StreamWrapper,
/// #     connect,
/// # };
/// #
/// # struct DummyHandler;
/// #
/// # impl Handler for DummyHandler {
/// #
/// #     async fn on_text(&mut self, text: &str) {
/// #         log::info!("on_text received: {text}");
/// #     }
/// #
/// #     async fn on_binary(&mut self, buffer: &[u8]) {
/// #         log::info!("on_binary received: {buffer:?}");
/// #     }
/// #
/// #     async fn on_close(&mut self, code: CloseCode, reason: &str) -> RetryStrategy {
/// #         log::info!("on_close received: {code:?}: {reason}");
/// #         RetryStrategy::Close
/// #     }
/// #
/// #     async fn on_connect(&mut self) {
/// #         log::info!("on_connect");
/// #     }
/// #
/// #     async fn on_connect_failure(&mut self) -> RetryStrategy {
/// #         log::info!("on_connect_failure");
/// #         RetryStrategy::Close
/// #     }
/// #
/// #     async fn on_disconnect(&mut self) -> RetryStrategy {
/// #         log::info!("on_disconnect");
/// #         RetryStrategy::Close
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
/// #     async fn connect() -> Result<
/// #         StreamWrapper<'static, Self::BackendStream, Self::BackendMessage, Self::Item, Self::Error>,
/// #         Self::Error,
/// #     > {
/// #         let response = reqwest::Client::default()
/// #             .get("ws://echo.websocket.org/")
/// #             .upgrade()
/// #             .send()
/// #             .await?;
/// #
/// #         response.into_websocket().await.map(StreamWrapper::from)
/// #     }
/// # }
/// # let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
/// # rt.block_on(async move {
///  let Some(client) = connect(DummyConnector, DummyHandler).await else {
///     log::info!("Failed to connect");
///     return;
///  };
///  
///  client.text("hello world").await.unwrap();
/// # });
/// ```
#[derive(Debug, Clone)]
pub struct Client {
    to_send: flume::Sender<Message>,
    command_tx: flume::Sender<Command>,
    confirm_close_rx: flume::Receiver<()>,
}

impl From<Client> for flume::Sender<Message> {
    #[inline]
    fn from(client: Client) -> Self {
        client.to_send
    }
}

impl Client {
    pub(crate) fn new(
        to_send: flume::Sender<Message>,
        command_tx: flume::Sender<Command>,
        confirm_close_rx: flume::Receiver<()>,
    ) -> Self {
        Self {
            to_send,
            command_tx,
            confirm_close_rx,
        }
    }

    /// Send a text message to the server.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendError) if all receivers have been dropped.
    #[inline]
    pub async fn text(&self, message: impl Into<String>) -> Result<(), flume::SendError<Message>> {
        let message = message.into();
        log::debug!("Sending text: {message}");

        self.to_send.send_async(Message::Text(message)).await
    }

    /// Send a text message to the server.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendError) if all receivers have been dropped.
    #[inline]
    pub fn blocking_text(
        &self,
        message: impl Into<String>,
    ) -> Result<(), flume::SendError<Message>> {
        let message = message.into();
        log::debug!("Sending text: {message}");

        self.to_send.send(Message::Text(message))
    }

    /// Send a text message to the server allowing to set up a timeout.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendTimeoutError) if all receivers have been dropped or the timeout has been reached.
    #[inline]
    pub fn blocking_text_timeout(
        &self,
        message: impl Into<String>,
        timeout: Duration,
    ) -> Result<(), flume::SendTimeoutError<Message>> {
        let message = message.into();
        log::debug!("Sending text: {message}");

        self.to_send.send_timeout(Message::Text(message), timeout)
    }

    /// Send a binary message to the server.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendError) if all receivers have been dropped.
    #[inline]
    pub async fn binary(
        &self,
        message: impl IntoIterator<Item = u8>,
    ) -> Result<(), flume::SendError<Message>> {
        let message = message.into_iter().collect();
        log::debug!("Sending binary: {message:?}");

        self.to_send.send_async(Message::Binary(message)).await
    }

    /// Send a binary message to the server.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendError) if all receivers have been dropped.
    #[inline]
    pub fn blocking_binary(
        &self,
        message: impl IntoIterator<Item = u8>,
    ) -> Result<(), flume::SendError<Message>> {
        let message = message.into_iter().collect();
        log::debug!("Sending binary: {message:?}");

        self.to_send.send(Message::Binary(message))
    }

    /// Send a binary message to the server.
    ///
    /// # Errors
    /// Returns an [`Error`](flume::SendTimeoutError) if all receivers have been dropped or the timeout has been reached.
    #[inline]
    pub fn blocking_binary_timeout(
        &self,
        message: impl IntoIterator<Item = u8>,
        timeout: Duration,
    ) -> Result<(), flume::SendTimeoutError<Message>> {
        let message = message.into_iter().collect();
        log::debug!("Sending text: {:?}", &message);

        self.to_send.send_timeout(Message::Binary(message), timeout)
    }

    /// Force the socket to reconnect, this can be useful when server address can change.
    ///
    /// # Errors
    /// Return an [`Error`](flume::SendError) if the receiver is dropped.
    #[inline]
    pub async fn force_reconnect(&self) -> Result<(), flume::SendError<Command>> {
        self.command_tx.send_async(Command::Reconnect).await
    }

    /// Force the socket to reconnect, this can be useful when server address can change.
    ///
    /// # Errors
    /// Return an [`Error`](flume::SendError) if the receiver is dropped.
    #[inline]
    pub fn blocking_force_reconnect(&self) -> Result<(), flume::SendError<Command>> {
        self.command_tx.send(Command::Reconnect)
    }

    /// Force the socket to reconnect, this can be useful when server address can change.
    ///
    /// # Errors
    /// Return an [`Error`](flume::SendTimeoutError) if the receiver is dropped.
    #[inline]
    pub fn blocking_force_reconnect_timeout(
        &self,
        timeout: Duration,
    ) -> Result<(), flume::SendTimeoutError<Command>> {
        self.command_tx.send_timeout(Command::Reconnect, timeout)
    }

    /// Allow the [`Client`] to close the connection.
    ///
    /// This will also stop any background task to avoid any reconnection.
    ///
    /// # Errors
    /// Return an [`Error`](flume::SendError) if the receiver is dropped.
    #[inline]
    pub async fn close(&self) -> IoResult<()> {
        self.command_tx
            .send_async(Command::Close)
            .await
            .map_err(|err| IoError::new(IoErrorKind::BrokenPipe, err))?;
        self.confirm_close_rx
            .recv_async()
            .await
            .map_err(|err| IoError::new(IoErrorKind::BrokenPipe, err))
    }

    /// Allow the [`Client`] to close the connection.
    ///
    /// This will also stop any background task to avoid any reconnection.
    ///
    /// # Errors
    /// Return an [`Error`](flume::SendError) if the receiver is dropped.
    #[inline]
    pub fn blocking_close(self) -> IoResult<()> {
        self.command_tx
            .send(Command::Close)
            .map_err(|err| IoError::new(IoErrorKind::BrokenPipe, err))?;
        self.confirm_close_rx
            .recv()
            .map_err(|err| IoError::new(IoErrorKind::BrokenPipe, err))
    }

    /// Allow the [`Client`] to close the connection.
    ///
    /// This will also stop any background task to avoid any reconnection.
    ///
    /// # Errors
    /// Return an [`Error`](flume::SendError) if the receiver is dropped.
    #[inline]
    pub fn blocking_close_timeout(
        self,
        request_timeout: Duration,
        confirmation_timeout: Duration,
    ) -> IoResult<()> {
        match self
            .command_tx
            .send_timeout(Command::Close, request_timeout)
        {
            Ok(()) => {}
            Err(reason) => {
                return match reason {
                    SendTimeoutError::Disconnected(_) => {
                        Err(IoError::new(IoErrorKind::BrokenPipe, reason))
                    }
                    SendTimeoutError::Timeout(_) => {
                        Err(IoError::new(IoErrorKind::TimedOut, reason))
                    }
                };
            }
        }
        self.confirm_close_rx
            .recv_timeout(confirmation_timeout)
            .map_err(|err| match err {
                RecvTimeoutError::Disconnected => IoError::new(IoErrorKind::BrokenPipe, err),
                RecvTimeoutError::Timeout => IoError::new(IoErrorKind::TimedOut, err),
            })
    }
}
