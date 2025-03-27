use crate::message::Message;
use futures::{SinkExt, StreamExt};
use std::time::Duration;

pub trait Connector: Send {
    type Item: From<Message> + Into<Message> + Send;
    type Stream: StreamExt<Item = Result<Self::Item, Self::Error>>
        + SinkExt<Self::Item>
        + Unpin
        + Send;
    type Error: std::error::Error + Send;

    fn connect() -> impl Future<Output = Result<Self::Stream, Self::Error>> + Send;

    #[must_use = "futures do nothing unless polled"]
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
