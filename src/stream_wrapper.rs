use futures::{Sink, SinkExt, Stream, StreamExt};
use std::{
    error::Error as StdError,
    marker::PhantomData,
    pin::{Pin, pin},
    task::{Context, Poll},
};

#[doc(hidden)]
pub struct StreamWrapper<'a, S, I, M, E>(S, PhantomData<&'a M>)
where
    S: StreamExt<Item = Result<I, E>> + SinkExt<I> + Unpin + Send,
    I: From<M> + Into<M> + Send,
    E: StdError;

impl<S, I, M, E> From<S> for StreamWrapper<'_, S, I, M, E>
where
    S: StreamExt<Item = Result<I, E>> + SinkExt<I> + Unpin + Send,
    I: From<M> + Into<M> + Send,
    E: StdError,
{
    #[inline]
    fn from(stream: S) -> Self {
        Self(stream, PhantomData)
    }
}

impl<S, I, M, E> Stream for StreamWrapper<'_, S, I, M, E>
where
    S: StreamExt<Item = Result<I, E>> + SinkExt<I> + Unpin + Send,
    E: StdError,
    I: From<M> + Into<M> + Send,
{
    type Item = Result<M, E>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match pin!(&mut self.0).poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Err(reason))) => Poll::Ready(Some(Err(reason))),
            Poll::Ready(Some(Ok(msg))) => Poll::Ready(Some(Ok(msg.into()))),
        }
    }
}

impl<S, I, M, E> Sink<M> for StreamWrapper<'_, S, I, M, E>
where
    S: StreamExt<Item = Result<I, E>> + SinkExt<I> + Unpin + Send,
    E: StdError,
    I: From<M> + Into<M> + Send,
{
    type Error = <S as Sink<I>>::Error;

    #[inline]
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(&mut self.0).poll_ready(cx)
    }

    #[inline]
    fn start_send(mut self: Pin<&mut Self>, item: M) -> Result<(), Self::Error> {
        pin!(&mut self.0).start_send(item.into())
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(&mut self.0).poll_flush(cx)
    }

    #[inline]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        pin!(&mut self.0).poll_close(cx)
    }
}
