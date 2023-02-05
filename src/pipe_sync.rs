use crate::database::DbSync;
use icepipe::pipe_stream::PipeStream;
use std::any::Any;

pub struct PipeSync<S: DbSync, P: PipeStream> {
    sync: S,
    pipe: P,
    pending: Option<PipeSyncPending>,
}
impl<S: DbSync, P: PipeStream> PipeSync<S, P> {
    pub fn new(sync: S, pipe: P) -> Self {
        Self {
            sync,
            pipe,
            pending: None,
        }
    }

    pub fn pre_wait(&mut self, database: &mut S::Database) {
        match &mut self.pending {
            Some(PipeSyncPending::Rx(message)) => {
                let message = std::mem::take(message);
                self.sync.rx(database, &message);
                self.pending = None;
                self.pre_wait(database)
            }
            Some(PipeSyncPending::Tx(_)) => {}
            None => {
                if let Some(message) = self.sync.tx(database) {
                    self.pending = Some(PipeSyncPending::Tx(message));
                }
            }
        }
    }

    pub async fn wait(&mut self) -> PipeSyncValue {
        match self.pending.take() {
            Some(PipeSyncPending::Rx(_)) => {
                unreachable!()
            }
            Some(PipeSyncPending::Tx(message)) => PipeSyncValue::Tx(message),
            None => PipeSyncValue::Rx(self.pipe.wait_dyn().await.unwrap()),
        }
    }

    pub async fn then(&mut self, value: PipeSyncValue) {
        assert!(self.pending.is_none());
        match value {
            PipeSyncValue::Rx(mut value) => {
                if let Some(message) = self.pipe.then_dyn(&mut value).await.unwrap() {
                    self.pending = Some(PipeSyncPending::Rx(message));
                }
            }
            PipeSyncValue::Tx(message) => {
                self.pipe.send(&message).await.unwrap();
            }
        }
    }
}

pub enum PipeSyncPending {
    Rx(Vec<u8>),
    Tx(Vec<u8>),
}

pub enum PipeSyncValue {
    Tx(Vec<u8>),
    Rx(Box<dyn Any>),
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{channel_pipe::ChannelPipe, database::DbSync};
    use std::time::Duration;

    struct CountSync {
        pos: usize,
        end: usize,
        wait: bool,
    }
    impl CountSync {
        pub fn new(data: &Vec<i32>, sender: bool) -> Self {
            let end = data.len();

            CountSync {
                pos: 0,
                end,
                wait: !sender,
            }
        }
    }
    impl DbSync for CountSync {
        type Database = Vec<i32>;

        fn tx(&mut self, database: &mut Self::Database) -> Option<Vec<u8>> {
            if self.wait || self.pos >= self.end {
                return None;
            }
            self.wait = true;
            let pos = self.pos;
            self.pos += 1;

            Some(vec![database[pos] as u8])
        }

        fn rx(&mut self, database: &mut Self::Database, message: &[u8]) {
            assert_eq!(message.len(), 1);
            self.wait = false;
            database.push(message[0] as i32);
        }
    }

    #[tokio::test]
    async fn sync_test() {
        let mut alice = vec![0, 2, 4];
        let mut bob = vec![1, 3, 5];
        let sync_a = CountSync::new(&alice, true);
        let sync_b = CountSync::new(&bob, false);

        let (pipe_a, pipe_b) = ChannelPipe::channel();

        let mut sync_a = PipeSync::new(sync_a, pipe_a);
        let mut sync_b = PipeSync::new(sync_b, pipe_b);

        loop {
            sync_a.pre_wait(&mut alice);
            sync_b.pre_wait(&mut bob);
            tokio::select! {
                value = sync_a.wait() => {
                    sync_a.then(value).await;
                }
                value = sync_b.wait() => {
                    sync_b.then(value).await;
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    break;
                }
            }
        }

        assert_eq!(alice, [0, 2, 4, 1, 3, 5]);
        assert_eq!(bob, [1, 3, 5, 0, 2, 4]);
    }
}
