use futures_util::FutureExt;
use icepipe::pipe_stream::{Control, PipeStream, WaitThen};

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
    fn send<'a>(&'a mut self, data: &'a [u8]) -> icepipe::PinFutureLocal<'a, ()> {
        async move {
            self.send
                .as_mut()
                .unwrap()
                .send(data.to_vec())
                .await
                .unwrap();

            Ok(())
        }
        .boxed_local()
    }
}
impl WaitThen for ChannelPipe {
    type Value = Option<Vec<u8>>;
    type Output = Option<Vec<u8>>;

    fn wait(&mut self) -> icepipe::PinFutureLocal<'_, Self::Value> {
        async move { Ok(self.recv.recv().await) }.boxed_local()
    }

    fn then<'a>(
        &'a mut self,
        value: &'a mut Self::Value,
    ) -> icepipe::PinFutureLocal<'a, Self::Output> {
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
    fn close(&mut self) -> icepipe::PinFutureLocal<'_, ()> {
        self.send = None;
        async move { Ok(()) }.boxed_local()
    }

    fn rx_closed(&self) -> bool {
        self.closed
    }
}
