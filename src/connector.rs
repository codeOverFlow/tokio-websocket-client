use crate::{StreamWrapper, message::Message};
use futures::{SinkExt, StreamExt};
use std::{error::Error as StdError, time::Duration};

pub type ConnectResult<C> = Result<
    StreamWrapper<
        'static,
        <C as Connector>::BackendStream,
        <C as Connector>::BackendMessage,
        <C as Connector>::Item,
        <C as Connector>::Error,
    >,
    <C as Connector>::Error,
>;
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
    type Error: StdError + Send;

    /// The function that will be used to connect/reconnect to the websocket server.
    fn connect() -> impl Future<Output = ConnectResult<Self>> + Send;

    /// The function that will provide the [`Duration`] between two retries.
    ///
    /// This is a function to allow implementors to dynamically compute the value
    /// allowing for back-off algorithm.
    #[inline]
    fn retry_delay(&mut self) -> impl Future<Output = Duration> + Send {
        async move { Duration::from_secs(5) }
    }

    /// The function that will provide the [`Duration`] between two ping messages.
    #[must_use]
    #[inline]
    fn ping_interval() -> Duration {
        Duration::from_secs(5)
    }

    /// The function that will provide the size for the request queue used to send
    /// requests to the background task sending them to the webserver.
    #[must_use]
    #[inline]
    fn request_queue_size() -> usize {
        500
    }
}
