use crate::database::{error::DatabaseError, DbSync};
use icepipe::pipe_stream::{PipeStream, StreamError};
use std::io;

pub struct PipeSync<S: DbSync, P>
where
    P: PipeStream,
    P::Error: Into<StreamError>,
{
    sync: S,
    pipe: P,
    pending: Option<PipeSyncPending>,
}
impl<S: DbSync, P> PipeSync<S, P>
where
    P: PipeStream,
    P::Error: Into<StreamError>,
{
    pub fn new(sync: S, pipe: P) -> Self {
        Self {
            sync,
            pipe,
            pending: None,
        }
    }

    pub async fn pre_wait(&mut self, database: &mut S::Database) -> PipeSyncResult<()> {
        loop {
            match &mut self.pending {
                Some(PipeSyncPending::Rx(message)) => {
                    let message = std::mem::take(message);
                    let message = bincode::deserialize(&message)?;
                    self.sync.rx(database, message).await?;
                    self.pending = None;
                    continue;
                }
                Some(PipeSyncPending::Tx(_)) => {}
                None => {
                    if let Some(message) = self.sync.tx(database).await? {
                        let message = bincode::serialize(&message)?;
                        self.pending = Some(PipeSyncPending::Tx(message));
                    }
                }
            }
            break;
        }

        Ok(())
    }

    pub async fn wait(&mut self) -> PipeSyncResult<PipeSyncValue<P>> {
        match self.pending.take() {
            Some(PipeSyncPending::Rx(_)) => {
                unreachable!()
            }
            Some(PipeSyncPending::Tx(message)) => Ok(PipeSyncValue::Tx(message)),
            None => Ok(PipeSyncValue::Rx(
                self.pipe.wait().await.map_err(Into::into)?,
            )),
        }
    }

    pub async fn then(&mut self, value: PipeSyncValue<P>) -> PipeSyncResult<()> {
        assert!(self.pending.is_none());
        match value {
            PipeSyncValue::Rx(mut value) => {
                if let Some(message) = self.pipe.then(&mut value).await.map_err(Into::into)? {
                    self.pending = Some(PipeSyncPending::Rx(message));
                }
            }
            PipeSyncValue::Tx(message) => {
                self.pipe.send(&message).await.map_err(Into::into)?;
            }
        }

        Ok(())
    }

    pub fn rx_closed(&self) -> bool {
        self.pipe.rx_closed()
    }

    pub async fn close(&mut self) -> PipeSyncResult<()> {
        self.pipe.close().await.map_err(Into::into)?;
        Ok(())
    }
}

pub enum PipeSyncPending {
    Rx(Vec<u8>),
    Tx(Vec<u8>),
}

pub enum PipeSyncValue<P>
where
    P: PipeStream,
    P::Error: Into<StreamError>,
{
    Tx(Vec<u8>),
    Rx(P::Value),
}

#[derive(thiserror::Error, Debug)]
pub enum PipeSyncError {
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    DatabaseError(#[from] DatabaseError),
    #[error(transparent)]
    StreamError(StreamError),
}
impl From<StreamError> for PipeSyncError {
    fn from(value: StreamError) -> Self {
        match value {
            StreamError::Io(e) => e.into(),
            e => Self::StreamError(e),
        }
    }
}
impl From<bincode::Error> for PipeSyncError {
    fn from(error: bincode::Error) -> Self {
        PipeSyncError::IoError(io::Error::new(io::ErrorKind::InvalidData, error))
    }
}
pub type PipeSyncResult<T> = Result<T, PipeSyncError>;

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::{
        channel_pipe::ChannelPipe,
        database::{error::DatabaseResult, DbSync},
    };
    use futures_util::{future::LocalBoxFuture, FutureExt};
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
        type Message = i32;

        fn tx<'a>(
            &'a mut self,
            database: &'a mut Self::Database,
        ) -> LocalBoxFuture<'a, DatabaseResult<Option<i32>>> {
            async move {
                if self.wait || self.pos >= self.end {
                    return Ok(None);
                }
                self.wait = true;
                let pos = self.pos;
                self.pos += 1;

                Ok(Some(database[pos]))
            }
            .boxed_local()
        }

        fn rx<'a>(
            &'a mut self,
            database: &'a mut Self::Database,
            message: i32,
        ) -> LocalBoxFuture<'a, DatabaseResult<()>> {
            async move {
                self.wait = false;
                database.push(message);

                Ok(())
            }
            .boxed_local()
        }
    }

    #[tokio::test]
    async fn sync_test() -> PipeSyncResult<()> {
        let mut alice = vec![0, 2, 4];
        let mut bob = vec![1, 3, 5];
        let sync_a = CountSync::new(&alice, true);
        let sync_b = CountSync::new(&bob, false);

        let (pipe_a, pipe_b) = ChannelPipe::channel();

        let mut sync_a = PipeSync::new(sync_a, pipe_a);
        let mut sync_b = PipeSync::new(sync_b, pipe_b);

        loop {
            sync_a.pre_wait(&mut alice).await?;
            sync_b.pre_wait(&mut bob).await?;
            tokio::select! {
                value = sync_a.wait() => {
                    sync_a.then(value?).await?;
                }
                value = sync_b.wait() => {
                    sync_b.then(value?).await?;
                }
                _ = tokio::time::sleep(Duration::from_millis(10)) => {
                    break;
                }
            }
        }

        assert_eq!(alice, [0, 2, 4, 1, 3, 5]);
        assert_eq!(bob, [1, 3, 5, 0, 2, 4]);

        Ok(())
    }
}
