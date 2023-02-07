use futures_util::{future::LocalBoxFuture, FutureExt};
use icepipe::pipe_stream::{Control, PipeStream, StreamError, WaitThen};
use std::io;

pub struct ChannelPipe {
    send: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
    recv: tokio::sync::mpsc::Receiver<Vec<u8>>,
    closed: bool,
}
impl ChannelPipe {
    pub fn channel() -> (ChannelPipe, ChannelPipe) {
        let a = tokio::sync::mpsc::channel(1);
        let b = tokio::sync::mpsc::channel(1);

        let a_pipe = ChannelPipe {
            send: Some(b.0),
            recv: a.1,
            closed: false,
        };

        let b_pipe = ChannelPipe {
            send: Some(a.0),
            recv: b.1,
            closed: false,
        };

        (a_pipe, b_pipe)
    }
}
impl PipeStream for ChannelPipe {
    fn send<'a>(&'a mut self, data: &'a [u8]) -> LocalBoxFuture<'a, ChannelPipeResult<()>> {
        async move {
            self.send
                .as_mut()
                .ok_or(ChannelPipeError::BrokenPipe)?
                .send(data.to_vec())
                .await
                .map_err(|_| ChannelPipeError::ConnectionReset)?;

            Ok(())
        }
        .boxed_local()
    }
}
impl WaitThen for ChannelPipe {
    type Value = Option<Vec<u8>>;
    type Output = Option<Vec<u8>>;
    type Error = ChannelPipeError;

    fn wait(&mut self) -> LocalBoxFuture<'_, ChannelPipeResult<Self::Value>> {
        async move { Ok(self.recv.recv().await) }.boxed_local()
    }

    fn then<'a>(
        &'a mut self,
        value: &'a mut Self::Value,
    ) -> LocalBoxFuture<'a, ChannelPipeResult<Self::Output>> {
        let value = value.take();

        async move {
            Ok(match value {
                Some(data) => Some(data),
                None => {
                    self.closed = true;
                    None
                }
            })
        }
        .boxed_local()
    }
}
impl Control for ChannelPipe {
    fn close(&mut self) -> LocalBoxFuture<'_, ChannelPipeResult<()>> {
        self.send = None;
        async move { Ok(()) }.boxed_local()
    }

    fn rx_closed(&self) -> bool {
        self.closed
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ChannelPipeError {
    #[error("This ChannelPipe was already closed")]
    BrokenPipe,
    #[error("Receiver side of this ChannelPipe was closed")]
    ConnectionReset,
}
impl From<ChannelPipeError> for io::Error {
    fn from(value: ChannelPipeError) -> Self {
        let kind = match value {
            ChannelPipeError::BrokenPipe => io::ErrorKind::BrokenPipe,
            ChannelPipeError::ConnectionReset => io::ErrorKind::ConnectionReset,
        };

        io::Error::new(kind, value)
    }
}
impl From<ChannelPipeError> for StreamError {
    fn from(value: ChannelPipeError) -> Self {
        StreamError::Io(value.into())
    }
}
pub type ChannelPipeResult<T> = Result<T, ChannelPipeError>;
