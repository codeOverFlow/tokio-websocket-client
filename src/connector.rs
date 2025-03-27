use crate::{StreamWrapper, message::Message};
use futures::{SinkExt, StreamExt};
use std::time::Duration;

/// Defines how to connect to the websocket server and provide some configuration.
pub trait Connector: Send
where
    <Self as Connector>::Item: 'static,
{
    /// The message that users implements to wrap their backend messages.
    type Item: From<Message> + Into<Message> + Send + Sync;
    /// The stream provided by users websocket backend.
    type BackendStream: StreamExt<Item = Result<Self::BackendMessage, Self::Error>>
        + SinkExt<Self::BackendMessage>
        + Unpin
        + Send;
    /// The message type provided by users websocket backend.
    type BackendMessage: From<Self::Item> + Into<Self::Item> + Send;
    /// The error type provided by users websocket backend.
    type Error: std::error::Error + Send;

    /// The function that will be used to connect/reconnect to the websocket server.
    #[allow(clippy::type_complexity)]
    fn connect() -> impl Future<
        Output = Result<
            StreamWrapper<
                'static,
                Self::BackendStream,
                Self::BackendMessage,
                Self::Item,
                Self::Error,
            >,
            Self::Error,
        >,
    > + Send;

    /// The function that will provide the [`Duration`] between two retries.
    ///
    /// This is a function to allow implementors to dynamically compute the value
    /// allowing for back-off algorithm.
    fn retry_delay(&mut self) -> impl Future<Output = Duration> + Send {
        async move { Duration::from_secs(5) }
    }

    /// The function that will provide the [`Duration`] between two ping messages.
    #[must_use]
    fn ping_interval() -> Duration {
        Duration::from_secs(5)
    }

    /// The function that will provide the size for the request queue used to send
    /// requests to the background task sending them to the webserver.
    #[must_use]
    fn request_queue_size() -> usize {
        500
    }
}
