use super::{error::DatabaseResult, DbSync};
use entity::crdt::Author;
use futures_util::{future::LocalBoxFuture, FutureExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

pub trait PatchDataSource {
    fn next(&mut self) -> LocalBoxFuture<DatabaseResult<Option<PatchData>>>;
    fn ack(&mut self, id: PatchDataId) -> LocalBoxFuture<DatabaseResult<()>>;
    fn merge(&mut self, data: PatchData) -> LocalBoxFuture<DatabaseResult<Option<PatchData>>>;
    fn save(&mut self, data: PatchData) -> LocalBoxFuture<DatabaseResult<()>>;
}

#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PatchData {
    id: PatchDataId,
    author: Author,
    conversation: Option<Uuid>,
}

pub type PatchDataId = i32;

pub struct PatchSync<S: PatchDataSource> {
    author: Author,
    conversation: Uuid,
    tx: VecDeque<PatchSyncMessage>,
    _marker: std::marker::PhantomData<S>,
}
impl<S: PatchDataSource> PatchSync<S> {
    pub fn new(source: &mut S, author: Author, conversation: Uuid) -> Self {
        let _ = source;
        PatchSync {
            author,
            conversation,
            tx: Default::default(),
            _marker: Default::default(),
        }
    }
}
impl<S: PatchDataSource> DbSync for PatchSync<S> {
    type Database = S;
    type Message = PatchSyncMessage;

    fn tx<'a>(
        &'a mut self,
        database: &'a mut Self::Database,
    ) -> LocalBoxFuture<'a, DatabaseResult<Option<Self::Message>>> {
        async move {
            loop {
                if let Some(next) = self.tx.pop_front() {
                    return Ok(Some(next));
                }

                let Some(next) = database.next().await? else {
                    return Ok(None);
                };

                let skip_by_conversation = next
                    .conversation
                    .map(|conversation| conversation != self.conversation)
                    .unwrap_or(false);

                if next.author == self.author || skip_by_conversation {
                    database.ack(next.id).await?;
                    continue;
                }
                break Ok(Some(PatchSyncMessage::Data(next)));
            }
        }
        .boxed_local()
    }

    fn rx<'a>(
        &'a mut self,
        database: &'a mut Self::Database,
        message: Self::Message,
    ) -> LocalBoxFuture<'a, DatabaseResult<()>> {
        async move {
            match message {
                PatchSyncMessage::Data(data) => {
                    let id = data.id;
                    if let Some(data) = database.merge(data).await? {
                        database.save(data).await?;
                    }
                    self.tx.push_back(PatchSyncMessage::Ack(id));
                }
                PatchSyncMessage::Ack(id) => database.ack(id).await?,
            }

            Ok(())
        }
        .boxed_local()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PatchSyncMessage {
    Data(PatchData),
    Ack(PatchDataId),
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashSet;

    use super::*;
    use rstest::*;

    #[derive(Clone, Default)]
    struct SourceMock {
        patches: Vec<PatchData>,
        minimum_ack: i32,
        merged: HashSet<PatchDataId>,
    }
    impl PatchDataSource for SourceMock {
        fn next(&mut self) -> LocalBoxFuture<DatabaseResult<Option<PatchData>>> {
            async move {
                let minimum = self.minimum_ack;
                Ok(self
                    .patches
                    .iter()
                    .find(|patch| patch.id > minimum)
                    .cloned())
            }
            .boxed_local()
        }

        fn ack(&mut self, id: PatchDataId) -> LocalBoxFuture<DatabaseResult<()>> {
            async move {
                self.minimum_ack = self.minimum_ack.max(id);

                Ok(())
            }
            .boxed_local()
        }

        fn merge(&mut self, data: PatchData) -> LocalBoxFuture<DatabaseResult<Option<PatchData>>> {
            async move {
                Ok(match self.merged.insert(data.id) {
                    true => Some(data),
                    false => None,
                })
            }
            .boxed_local()
        }

        fn save(&mut self, data: PatchData) -> LocalBoxFuture<DatabaseResult<()>> {
            async move {
                self.patches.push(data);
                Ok(())
            }
            .boxed_local()
        }
    }

    mod given_a_patch_sync {
        use super::*;

        type Given = (SourceMock, PatchSync<SourceMock>);
        #[fixture]
        fn given() -> Given {
            let mut source = Default::default();
            let sync = PatchSync::new(&mut source, Author(3), Uuid::from_u128(3));

            (source, sync)
        }

        #[rstest]
        #[tokio::test]
        async fn and_there_is_a_pending_patch_then_it_sends_the_patch_as_a_tx_message(
            given: Given,
        ) {
            let (mut source, mut sync, ..) = given;
            let data = PatchData {
                id: 37,
                author: Author(5),
                ..Default::default()
            };
            source.patches = vec![data];

            let tx = sync.tx(&mut source).await.unwrap();
            assert_eq!(tx, Some(PatchSyncMessage::Data(source.patches[0].clone())));
        }

        mod when_it_receives_a_patch {
            use super::*;

            type Given = (SourceMock, PatchSync<SourceMock>, PatchData);
            async fn given() -> Given {
                let (mut source, mut sync) = super::given();
                let data = PatchData {
                    id: 37,
                    author: Author(5),
                    ..Default::default()
                };

                sync.rx(&mut source, PatchSyncMessage::Data(data.clone()))
                    .await
                    .unwrap();

                (source, sync, data)
            }

            #[tokio::test]
            async fn then_it_sends_an_acknowledge() {
                let (mut source, mut sync, data, ..) = given().await;

                let message = sync.tx(&mut source).await.unwrap();

                assert_eq!(message, Some(PatchSyncMessage::Ack(data.id)));
            }

            #[tokio::test]
            async fn then_it_merges_the_patch() {
                let (source, _, data, ..) = given().await;

                assert_eq!(source.merged.into_iter().collect::<Vec<_>>(), vec![data.id]);
            }

            #[tokio::test]
            async fn then_it_and_saves_the_patch() {
                let (source, _, data, ..) = given().await;

                assert_eq!(source.patches, vec![data]);
            }
        }

        mod when_it_receives_a_repeated_patch {
            use super::*;

            type Given = (SourceMock, PatchSync<SourceMock>, PatchData);
            async fn given() -> Given {
                let (mut source, mut sync) = super::given();
                let data = PatchData {
                    id: 37,
                    author: Author(5),
                    ..Default::default()
                };

                source.merged.insert(data.id);
                sync.rx(&mut source, PatchSyncMessage::Data(data.clone()))
                    .await
                    .unwrap();

                (source, sync, data)
            }

            #[tokio::test]
            async fn then_it_acknowledges_it() {
                let (mut source, mut sync, data, ..) = given().await;

                let ack = sync.tx(&mut source).await.unwrap();
                assert_eq!(ack, Some(PatchSyncMessage::Ack(data.id)));
            }

            #[tokio::test]
            async fn then_it_doesnt_save_it() {
                let (source, ..) = given().await;

                assert_eq!(source.patches, vec![]);
            }
        }

        #[rstest]
        #[tokio::test]
        async fn when_it_receives_an_ack_message_it_sets_it_on_database(given: Given) {
            let (mut source, mut sync, ..) = given;

            let ack = PatchSyncMessage::Ack(3);
            sync.rx(&mut source, ack).await.unwrap();

            assert_eq!(source.minimum_ack, 3);
        }

        mod when_next_patch_is_from_the_peer_itself {
            use super::*;

            type Given = (SourceMock, PatchSync<SourceMock>, PatchData);
            #[fixture]
            fn given() -> Given {
                let (mut source, sync) = super::given();
                let data = PatchData {
                    id: 37,
                    author: Author(3),
                    ..Default::default()
                };
                source.patches.push(data.clone());

                (source, sync, data)
            }

            #[rstest]
            #[tokio::test]
            async fn then_it_doesnt_send_it(given: Given) {
                let (mut source, mut sync, ..) = given;

                let tx = sync.tx(&mut source).await.unwrap();

                assert_eq!(tx, None);
            }

            #[rstest]
            #[tokio::test]
            async fn then_patch_is_marked_as_handled(given: Given) {
                let (mut source, mut sync, data, ..) = given;

                sync.tx(&mut source).await.unwrap();

                assert_eq!(source.minimum_ack, data.id);
            }

            #[rstest]
            #[tokio::test]
            async fn and_there_is_another_patch_in_the_list_then_it_sends_the_second_patch(
                given: Given,
            ) {
                let (mut source, mut sync, data, ..) = given;

                let another = PatchData {
                    id: data.id + 1,
                    author: Author(5),
                    ..data.clone()
                };
                source.patches.push(another.clone());

                let tx = sync.tx(&mut source).await.unwrap();

                assert_eq!(tx, Some(PatchSyncMessage::Data(another)));
            }
        }

        mod when_next_patch_specifies_a_different_conversation_than_the_channels_conversation {
            use super::*;

            type Given = (SourceMock, PatchSync<SourceMock>, PatchData);
            #[fixture]
            fn given() -> Given {
                let (mut source, sync) = super::given();

                let data = PatchData {
                    id: 37,
                    author: Author(5),
                    conversation: Some(Uuid::from_u128(5)),
                };

                source.patches.push(data.clone());

                (source, sync, data)
            }

            #[rstest]
            #[tokio::test]
            async fn then_the_patch_is_not_sent(given: Given) {
                let (mut source, mut sync, ..) = given;

                let tx = sync.tx(&mut source).await.unwrap();
                assert_eq!(tx, None);
            }

            #[rstest]
            #[tokio::test]
            async fn then_patch_is_marked_as_handled(given: Given) {
                let (mut source, mut sync, data, ..) = given;

                sync.tx(&mut source).await.unwrap();
                assert_eq!(source.minimum_ack, data.id)
            }

            #[rstest]
            #[tokio::test]
            async fn and_there_is_a_pending_patch_then_it_sends_the_patch_as_a_tx_message(
                given: Given,
            ) {
                let (mut source, mut sync, data, ..) = given;

                let another = PatchData {
                    id: data.id + 1,
                    conversation: Some(Uuid::from_u128(3)),
                    ..data.clone()
                };
                source.patches.push(another.clone());

                let tx = sync.tx(&mut source).await.unwrap();
                assert_eq!(tx, Some(PatchSyncMessage::Data(another)));
            }
        }
    }
}
