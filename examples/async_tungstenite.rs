use tokio_websocket_client::{
    CloseCode,
    Connector,
    Handler,
    Message,
    RetryStrategy,
    StreamWrapper,
    connect,
};

struct DummyHandler;

impl Handler for DummyHandler {
    async fn on_text(&mut self, text: &str) {
        log::info!("on_text received: {text}");
    }

    async fn on_binary(&mut self, buffer: &[u8]) {
        log::info!("on_binary received: {buffer:?}");
    }

    async fn on_close(&mut self, code: CloseCode, reason: &str) -> RetryStrategy {
        log::info!("on_close received: {code:?}: {reason}");
        RetryStrategy::Close
    }

    async fn on_connect(&mut self) {
        log::info!("on_connect");
    }

    async fn on_connect_failure(&mut self) -> RetryStrategy {
        log::info!("on_connect_failure");
        RetryStrategy::Close
    }

    async fn on_disconnect(&mut self) -> RetryStrategy {
        log::info!("on_disconnect");
        RetryStrategy::Close
    }
}

struct DummyMessage(async_tungstenite::tungstenite::Message);

impl From<async_tungstenite::tungstenite::Message> for DummyMessage {
    fn from(message: async_tungstenite::tungstenite::Message) -> Self {
        Self(message)
    }
}

impl From<DummyMessage> for async_tungstenite::tungstenite::Message {
    fn from(message: DummyMessage) -> Self {
        message.0
    }
}

impl From<DummyMessage> for Message {
    fn from(other: DummyMessage) -> Message {
        match other {
            DummyMessage(async_tungstenite::tungstenite::Message::Text(data)) => {
                Message::Text(data.to_string())
            }
            DummyMessage(async_tungstenite::tungstenite::Message::Binary(data)) => {
                Message::Binary(data.into())
            }
            DummyMessage(async_tungstenite::tungstenite::Message::Ping(data)) => {
                Message::Ping(data.into())
            }
            DummyMessage(async_tungstenite::tungstenite::Message::Pong(data)) => {
                Message::Pong(data.into())
            }
            DummyMessage(async_tungstenite::tungstenite::Message::Close(frame)) => {
                if let Some(frame) = frame {
                    Message::Close(
                        CloseCode::from(u16::from(frame.code)),
                        frame.reason.to_string(),
                    )
                } else {
                    Message::Close(CloseCode::Normal, String::default())
                }
            }
            // async-tungstenite defines a Message::Frame, but it is not returned when reading
            _ => unreachable!(),
        }
    }
}

impl From<Message> for DummyMessage {
    fn from(msg: Message) -> Self {
        match msg {
            Message::Text(data) => Self(async_tungstenite::tungstenite::Message::Text(data.into())),
            Message::Binary(data) => {
                Self(async_tungstenite::tungstenite::Message::Binary(data.into()))
            }
            Message::Ping(data) => Self(async_tungstenite::tungstenite::Message::Ping(data.into())),
            Message::Pong(data) => Self(async_tungstenite::tungstenite::Message::Pong(data.into())),
            Message::Close(code, reason) => Self(async_tungstenite::tungstenite::Message::Close(
                Some(async_tungstenite::tungstenite::protocol::CloseFrame {
                    code: async_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(
                        u16::from(code),
                    ),
                    reason: reason.into(),
                }),
            )),
        }
    }
}

struct DummyConnector;

impl Connector for DummyConnector {
    type Item = DummyMessage;
    type BackendStream =
        async_tungstenite::WebSocketStream<async_tungstenite::tokio::ConnectStream>;
    type BackendMessage = async_tungstenite::tungstenite::Message;
    type Error = async_tungstenite::tungstenite::Error;

    async fn connect() -> Result<
        StreamWrapper<'static, Self::BackendStream, Self::BackendMessage, Self::Item, Self::Error>,
        Self::Error,
    > {
        let (stream, response) =
            async_tungstenite::tokio::connect_async("ws://echo.websocket.org").await?;
        dbg!(response);
        Ok(StreamWrapper::from(stream))
    }
}

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Trace).unwrap();
    let Some(client) = connect(DummyConnector, DummyHandler).await else {
        log::info!("Failed to connect");
        return;
    };

    client.text("hello world").await.unwrap();
    client.close().await.unwrap();
}
