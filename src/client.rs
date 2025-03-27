use crate::Message;
use std::time::Duration;

/// A connected client that can be used to send messages to the server.
///
/// # Example
/// ```
/// # use std::pin::{pin, Pin};
/// # use std::task::{Context, Poll};
/// # use std::time::Duration;
/// # use futures::{Sink, SinkExt, Stream, StreamExt};
/// # use reqwest_websocket;
/// # use reqwest_websocket::RequestBuilderExt;
/// # use tokio_websocket_client::{connect, CloseCode, Connector, Handler, Message, RetryStrategy};
/// #
/// # // Create a basic handler for server messages
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
/// #     async fn on_close(&mut self, code: CloseCode, reason: &str) -> Result<RetryStrategy, Self::Error> {
/// #         log::info!("on_close received: {code:?}: {reason}");
/// #         Ok(RetryStrategy::Close)
/// #     }
/// # }
/// #
/// # // Wraps reqwest_websocket::Message into our own type in order to implements needed traits
/// # struct DummyMessage(reqwest_websocket::Message);
/// #
/// # impl From<reqwest_websocket::Message> for DummyMessage {
/// #     fn from(message: reqwest_websocket::Message) -> Self {
/// #         Self(message)
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
/// #             DummyMessage(reqwest_websocket::Message::Close {code, reason}) => Message::Close(CloseCode::from(u16::from(code)), reason),
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
/// #             Message::Close(code , reason) => Self(reqwest_websocket::Message::Close { code: reqwest_websocket::CloseCode::from(u16::from(code)), reason }),
/// #         }
/// #     }
/// # }
/// #
/// # // Wraps reqwest_websocket::WebSocket to delegate implementation over our DummyMessage struct
/// # struct DummyStream(reqwest_websocket::WebSocket);
/// #
/// # impl Stream for DummyStream {
/// #     type Item = Result<DummyMessage, reqwest_websocket::Error>;
/// #
/// #     fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
/// #         match pin!(&mut self.0).poll_next(cx) {
/// #             Poll::Pending => Poll::Pending,
/// #             Poll::Ready(None) => Poll::Ready(None),
/// #             Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
/// #             Poll::Ready(Some(Ok(msg))) => {Poll::Ready(Some(Ok(msg.into())))}
/// #         }
/// #     }
/// # }
/// #
/// # impl Sink<DummyMessage> for DummyStream {
/// #     type Error = reqwest_websocket::Error;
/// #
/// #     fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
/// #         pin!(&mut self.0).poll_ready(cx)
/// #     }
/// #
/// #     fn start_send(mut self: Pin<&mut Self>, item: DummyMessage) -> Result<(), Self::Error> {
/// #         pin!(&mut self.0).start_send(item.0)
/// #     }
/// #
/// #     fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
/// #         pin!(&mut self.0).poll_flush(cx)
/// #     }
/// #
/// #     fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
/// #         pin!(&mut self.0).poll_close(cx)
/// #     }
/// # }
/// #
/// # // Create a basic Connector that will define how to connect/reconnect to the server
/// # struct DummyConnector;
/// #
/// # impl Connector for DummyConnector {
/// #     type Item = DummyMessage;
/// #     type Stream = DummyStream;
/// #     type Error = reqwest_websocket::Error;
/// #
/// #     async fn connect() -> Result<Self::Stream, Self::Error> {
/// #         // Creates a GET request, upgrades and sends it.
/// #         let response = reqwest::Client::default()
/// #             .get("wss://echo.websocket.org/")
/// #             .upgrade() //  Prepares the WebSocket upgrade.
/// #             .send()
/// #             .await?;
/// #
/// #         // Turns the response into a DummyStream.
/// #         response.into_websocket().await.map(DummyStream)
/// #     }
/// # }
/// #
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
    pub async fn text(&self, message: impl Into<String>) -> Result<(), flume::SendError<Message>> {
        let message = message.into();
        log::debug!("Sending text: {}", &message);

        self.to_send.send_async(Message::Text(message)).await
    }

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

    pub async fn binary(
        &self,
        message: impl IntoIterator<Item = u8>,
    ) -> Result<(), flume::SendError<Message>> {
        let message = message.into_iter().collect();
        log::debug!("Sending text: {:?}", &message);

        self.to_send.send_async(Message::Binary(message)).await
    }

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
