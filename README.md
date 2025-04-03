[![crates.io](https://img.shields.io/crates/v/tokio-websocket-client)](https://crates.io/crates/tokio-websocket-client)
[![MIT licensed](https://img.shields.io/badge/license-MIT-blue)](LICENSE-MIT)

# tokio-websocket-client
`tokio-websocket-client` is a tokio based websocket client. It aims to ease dealing with websockets.

## Features
- Ping/Pong
- Automatic reconnection
- Retry strategy
- Generic so you can use any websocket backend

## Example
This is a small example on how to use this crate using the `reqwest_websocket` backend.
```rust
use reqwest_websocket::RequestBuilderExt;
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

struct DummyMessage(reqwest_websocket::Message);

impl From<reqwest_websocket::Message> for DummyMessage {
    fn from(message: reqwest_websocket::Message) -> Self {
        Self(message)
    }
}

impl From<DummyMessage> for reqwest_websocket::Message {
    fn from(message: DummyMessage) -> Self {
        message.0
    }
}

impl From<DummyMessage> for Message {
    fn from(other: DummyMessage) -> Message {
        match other {
            DummyMessage(reqwest_websocket::Message::Text(data)) => Message::Text(data),
            DummyMessage(reqwest_websocket::Message::Binary(data)) => Message::Binary(data),
            DummyMessage(reqwest_websocket::Message::Ping(data)) => Message::Ping(data),
            DummyMessage(reqwest_websocket::Message::Pong(data)) => Message::Pong(data),
            DummyMessage(reqwest_websocket::Message::Close { code, reason }) => {
                Message::Close(CloseCode::from(u16::from(code)), reason)
            }
        }
    }
}

impl From<Message> for DummyMessage {
    fn from(msg: Message) -> Self {
        match msg {
            Message::Text(data) => Self(reqwest_websocket::Message::Text(data)),
            Message::Binary(data) => Self(reqwest_websocket::Message::Binary(data)),
            Message::Ping(data) => Self(reqwest_websocket::Message::Ping(data)),
            Message::Pong(data) => Self(reqwest_websocket::Message::Pong(data)),
            Message::Close(code, reason) => Self(reqwest_websocket::Message::Close {
                code: reqwest_websocket::CloseCode::from(u16::from(code)),
                reason,
            }),
        }
    }
}

struct DummyConnector;

impl Connector for DummyConnector {
    type Item = DummyMessage;
    type BackendStream = reqwest_websocket::WebSocket;
    type BackendMessage = reqwest_websocket::Message;
    type Error = reqwest_websocket::Error;

    async fn connect() -> Result<
        StreamWrapper<'static, Self::BackendStream, Self::BackendMessage, Self::Item, Self::Error>,
        Self::Error,
    > {
        // Creates a GET request, upgrades and sends it.
        let response = reqwest::Client::default()
            .get("ws://echo.websocket.org/")
            .upgrade() // Prepares the WebSocket upgrade.
            .send()
            .await?;

        // Turns the response into a DummyStream.
        response.into_websocket().await.map(StreamWrapper::from)
    }
}

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Trace).unwrap();
    
    // Wait for the 1st connection to succeed or for the handler to stop retrying.
    let Some(client) = connect(DummyConnector, DummyHandler).await else {
        log::info!("Failed to connect");
        return;
    };

    client.text("hello world").await.unwrap();
}
```