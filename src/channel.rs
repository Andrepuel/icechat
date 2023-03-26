use crate::{
    database::{ChannelData, DbSync},
    fragmentable::Fragmentable,
    pipe_sync::{PipeSync, PipeSyncResult, PipeSyncValue},
};
use entity::crdt::Author;
use futures_util::{future::LocalBoxFuture, FutureExt};
use icepipe::{
    agreement::Ed25519PairAndPeer,
    connect::{ConnectResult, Connection},
    pipe_stream::StreamError,
};
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::ops::Deref;

pub struct Channel<S: DbSync> {
    channel: ChannelData,
    key: Ed25519Seed,
    state: ChannelState<S>,
}
impl<S: DbSync> Channel<S> {
    pub fn new(channel: ChannelData, key: Ed25519Seed) -> Self {
        Self {
            channel,
            key,
            state: Default::default(),
        }
    }

    pub fn connect(&mut self, state: S) {
        self.state = ChannelState::PreConnecting(state);
    }

    pub fn channel(&self) -> &ChannelData {
        &self.channel
    }

    pub fn state(&self) -> ChannelStateLabel {
        self.state.label()
    }

    pub async fn pre_wait(&mut self, database: &mut S::Database) {
        let state = std::mem::take(&mut self.state);

        let r = Self::pre_wait_impl(state, database).await;

        match r {
            Ok(state) => {
                self.state = state;
            }
            Err(e) => {
                log::warn!("{e}");
                log::debug!("{e:?}");
            }
        };
    }

    async fn pre_wait_impl(
        state: ChannelState<S>,
        database: &mut S::Database,
    ) -> PipeSyncResult<ChannelState<S>> {
        match state {
            ChannelState::Connected(mut sync) => {
                if sync.rx_closed() {
                    return Ok(ChannelState::Offline);
                }

                sync.pre_wait(database).await?;
                Ok(ChannelState::Connected(sync))
            }
            state => Ok(state),
        }
    }

    pub async fn wait(&mut self) -> ChannelValue {
        let r = self.wait_impl().await;
        match r {
            Ok(value) => value,
            Err(e) => {
                self.state = ChannelState::Offline;
                log::warn!("{e}");
                log::debug!("{e:?}");
                ChannelValue::Error
            }
        }
    }

    async fn wait_impl(&mut self) -> PipeSyncResult<ChannelValue> {
        match &mut self.state {
            ChannelState::Offline => {
                std::future::pending::<()>().await;
                unreachable!()
            }
            ChannelState::PreConnecting(_) => {
                let key = self.key.key_pair();
                let auth = Ed25519PairAndPeer(key, self.channel.peer_cert.0.to_vec());
                Ok(ChannelValue::StartConnection(
                    self.channel.channel.to_string(),
                    auth,
                ))
            }
            ChannelState::Connecting(_, connecting) => {
                let pipe = connecting.await.map_err(StreamError::from)?;
                let state = std::mem::take(&mut self.state);
                let sync = match state {
                    ChannelState::Connecting(sync_state, _) => sync_state,
                    _ => unreachable!(),
                };
                let pipe_sync = PipeSync::new(sync, Fragmentable::new(pipe));
                self.state = ChannelState::Connected(pipe_sync);

                Ok(ChannelValue::Connected)
            }
            ChannelState::Connected(pipe_sync) => {
                let value = pipe_sync.wait().await?;
                Ok(ChannelValue::PipeSyncValue(value))
            }
        }
    }

    pub async fn then(&mut self, value: ChannelValue) {
        let r = self.then_impl(value).await;

        match r {
            Ok(()) => {}
            Err(e) => {
                self.state = ChannelState::Offline;
                log::warn!("{e}");
                log::debug!("{e:?}");
            }
        }
    }

    pub async fn then_impl(&mut self, value: ChannelValue) -> PipeSyncResult<()> {
        match (&mut self.state, value) {
            (ChannelState::PreConnecting(_), ChannelValue::StartConnection(channel, auth)) => {
                let state = std::mem::take(&mut self.state);
                let sync = match state {
                    ChannelState::PreConnecting(sync) => sync,
                    _ => unreachable!(),
                };

                let connecting = async move {
                    icepipe::ConnectOptions {
                        channel,
                        signaling: Default::default(),
                        ice: Default::default(),
                    }
                    .connect(auth)
                    .await
                }
                .boxed_local();
                self.state = ChannelState::Connecting(sync, connecting);

                Ok(())
            }
            (ChannelState::Connected(pipe_sync), ChannelValue::PipeSyncValue(value)) => {
                pipe_sync.then(value).await
            }
            _ => Ok(()),
        }
    }

    pub async fn close(&mut self) {
        let r = Self::close_impl(std::mem::take(&mut self.state)).await;

        if let Err(e) = r {
            log::warn!("{e}");
            log::debug!("{e:?}");
        }
    }

    async fn close_impl(state: ChannelState<S>) -> PipeSyncResult<()> {
        match state {
            ChannelState::Offline => Ok(()),
            ChannelState::PreConnecting(_) => Ok(()),
            ChannelState::Connecting(_, _) => Ok(()),
            ChannelState::Connected(mut pipe_sync) => pipe_sync.close().await,
        }
    }
}

#[derive(Clone)]
pub struct Ed25519Seed([u8; 32]);
impl Ed25519Seed {
    pub fn new(seed: [u8; 32]) -> Ed25519Seed {
        Self(seed)
    }

    pub fn generate() -> Ed25519Seed {
        let seed = ring::rand::generate(&ring::rand::SystemRandom::new())
            .unwrap()
            .expose();
        Ed25519Seed(seed)
    }

    pub fn public_key(&self) -> Ed25519Cert {
        let key_pair = self.key_pair();
        Ed25519Cert(key_pair.public_key().as_ref().try_into().unwrap())
    }

    pub fn key_pair(&self) -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&self.0).unwrap()
    }

    pub fn x25519_agree(&self, salt: &str, peer_cert: &Ed25519Cert) -> String {
        let x25519_user = icepipe::curve25519_conversion::ed25519_seed_to_x25519(self.0.as_slice());
        let x25519_peer =
            icepipe::curve25519_conversion::ed25519_public_key_to_x25519(peer_cert.0.as_slice())
                .unwrap();

        let secret = x25519_user.diffie_hellman(&x25519_peer);
        let basekey = secret
            .as_bytes()
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect::<String>();

        icepipe::agreement::PskAuthentication::derive_text(&basekey, salt)
    }
}
impl Deref for Ed25519Seed {
    type Target = [u8; 32];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ed25519Cert(pub [u8; 32]);
impl Ed25519Cert {
    pub fn as_author(&self) -> Author {
        Author(
            i32::from_le_bytes((&self.0[0..][..4]).try_into().unwrap())
                ^ i32::from_le_bytes((&self.0[4..][..4]).try_into().unwrap())
                ^ i32::from_le_bytes((&self.0[8..][..4]).try_into().unwrap())
                ^ i32::from_le_bytes((&self.0[12..][..4]).try_into().unwrap()),
        )
    }

    pub fn hex(&self) -> String {
        self.0
            .iter()
            .map(|c| format!("{c:02x}"))
            .collect::<String>()
    }
}

pub enum ChannelState<S: DbSync> {
    Offline,
    PreConnecting(S),
    Connecting(S, LocalBoxFuture<'static, ConnectResult<Connection>>),
    Connected(PipeSync<S, Fragmentable<Connection>>),
}
impl<S: DbSync> Default for ChannelState<S> {
    fn default() -> Self {
        Self::Offline
    }
}
impl<S: DbSync> ChannelState<S> {
    fn label(&self) -> ChannelStateLabel {
        match self {
            ChannelState::Offline => ChannelStateLabel::Offline,
            ChannelState::PreConnecting(_) => ChannelStateLabel::PreConnecting,
            ChannelState::Connecting(_, _) => ChannelStateLabel::Connecting,
            ChannelState::Connected(_) => ChannelStateLabel::Connected,
        }
    }
}

pub enum ChannelValue {
    Connected,
    StartConnection(String, Ed25519PairAndPeer),
    PipeSyncValue(PipeSyncValue<Fragmentable<Connection>>),
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChannelStateLabel {
    Offline,
    PreConnecting,
    Connecting,
    Connected,
}
