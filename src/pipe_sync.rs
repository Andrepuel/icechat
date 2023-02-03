use crate::database::DbSync;
use icepipe::pipe_stream::PipeStream;

pub struct PipeSync<S: DbSync, P: PipeStream> {
    sync: S,
    pipe: P,
}
impl<S: DbSync, P: PipeStream> PipeSync<S, P> {
    pub fn new(sync: S, pipe: P) -> Self {
        Self { sync, pipe }
    }

    pub async fn run(mut self) {
        let mut tx_closed: bool = false;
        while !tx_closed && !self.pipe.rx_closed() {
            let send = self.sync.message();
            let sent = send.is_some();

            if let Some(message) = send {
                self.pipe.send(message).await.unwrap();
                self.sync.pop_message();
            } else {
                self.pipe.send(&[]).await.unwrap();
            }

            let received;
            if !self.pipe.rx_closed() {
                let mut wait = self.pipe.wait_dyn().await.unwrap();
                let message = self
                    .pipe
                    .then_dyn(&mut wait)
                    .await
                    .unwrap()
                    .unwrap_or_default();

                if !message.is_empty() {
                    self.sync.receive(&message);
                    received = true;
                } else {
                    received = false;
                }
            } else {
                received = false;
            }

            if !sent && !received {
                self.pipe.close().await.unwrap();
                tx_closed = true;
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{channel_pipe::ChannelPipe, database::DbSync};
    use futures_util::future::join_all;

    struct CountSync<'a> {
        data: &'a mut Vec<i32>,
        pos: usize,
        end: usize,
        wait: bool,
        buffer: u8,
    }
    impl<'a> CountSync<'a> {
        pub fn new(data: &'a mut Vec<i32>, sender: bool) -> Self {
            let end = data.len();
            let buffer = data.first().copied().unwrap_or(0) as u8;

            CountSync {
                data,
                pos: 0,
                end,
                wait: !sender,
                buffer,
            }
        }
    }
    impl<'a> DbSync for CountSync<'a> {
        fn message(&self) -> Option<&[u8]> {
            if self.wait || self.pos >= self.end {
                return None;
            }

            Some(std::slice::from_ref(&self.buffer))
        }

        fn pop_message(&mut self) {
            self.pos += 1;
            self.wait = true;
        }

        fn receive(&mut self, message: &[u8]) {
            assert_eq!(message.len(), 1);
            self.wait = false;
            self.buffer = self.data.get(self.pos).copied().unwrap_or_default() as u8;
            self.data.push(message[0] as i32);
        }

        fn done(self) {}
    }

    #[tokio::test]
    async fn sync_test() {
        let mut alice = vec![0, 2, 4];
        let mut bob = vec![1, 3, 5];
        let sync_a = CountSync::new(&mut alice, true);
        let sync_b = CountSync::new(&mut bob, true);

        let (pipe_a, pipe_b) = ChannelPipe::channel();

        let sync_a = PipeSync::new(sync_a, pipe_a);
        let sync_b = PipeSync::new(sync_b, pipe_b);

        join_all([sync_a.run(), sync_b.run()]).await;

        assert_eq!(alice, [0, 2, 4, 1, 3, 5]);
        assert_eq!(bob, [1, 3, 5, 0, 2, 4]);
    }
}
