use crate::{StreamWrapper, message::Message};
use futures::{SinkExt, StreamExt};
use std::time::Duration;

pub trait Connector: Send
where
    <Self as Connector>::Item: 'static,
{
    type Item: From<Message> + Into<Message> + Send + Sync;
    type BackendStream: StreamExt<Item = Result<Self::BackendMessage, Self::Error>>
        + SinkExt<Self::BackendMessage>
        + Unpin
        + Send;
    type BackendMessage: From<Self::Item> + Into<Self::Item> + Send;
    type Error: std::error::Error + Send;

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

    fn retry_delay(&mut self) -> impl Future<Output = Duration> + Send {
        async move { Duration::from_secs(5) }
    }

    #[must_use]
    fn ping_interval() -> Duration {
        Duration::from_secs(5)
    }

    #[must_use]
    fn request_queue_size() -> usize {
        500
    }
}
