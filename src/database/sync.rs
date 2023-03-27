use super::{error::DatabaseResult, DbSync};
use entity::{crdt::Author, patch::Patch};
use futures_util::{future::LocalBoxFuture, FutureExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

pub trait SyncDataSource {
    type Ctx: Copy;

    fn next(
        &mut self,
        ctx: Self::Ctx,
        minimum: (i32, i32),
    ) -> LocalBoxFuture<DatabaseResult<Option<SyncData>>>;
    fn ack(&mut self, ctx: Self::Ctx, id: SyncDataId) -> LocalBoxFuture<DatabaseResult<()>>;
    fn merge(
        &mut self,
        ctx: Self::Ctx,
        data: SyncData,
    ) -> LocalBoxFuture<DatabaseResult<Option<SyncData>>>;
    fn save(&mut self, ctx: Self::Ctx, data: SyncData) -> LocalBoxFuture<DatabaseResult<()>>;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct SyncData {
    pub id: SyncDataId,
    pub payload: Patch,
}
impl Default for SyncData {
    fn default() -> Self {
        SyncData {
            id: Default::default(),
            payload: Patch::Contact(Default::default()),
        }
    }
}
impl SyncData {
    pub fn conversation(&self) -> Option<Uuid> {
        match &self.payload {
            Patch::Contact(_) => None,
            Patch::Conversation(conversation) => Some(conversation.id),
            Patch::Member(member) => Some(member.conversation),
            Patch::NewTextMessage(message) => Some(message.conversation),
            Patch::MessageStatus(status) => Some(status.conversation),
            Patch::Attachment(attachment) => Some(attachment.conversation),
            Patch::NewAttachmentMessage(attachment) => Some(attachment.conversation),
        }
    }

    pub fn author(&self) -> Author {
        match &self.payload {
            Patch::Contact(contact) => contact.crdt.author,
            Patch::Conversation(conversation) => conversation.crdt.author,
            Patch::Member(member) => member.crdt.0,
            Patch::NewTextMessage(message) => message.crdt.writable.author,
            Patch::MessageStatus(message) => message.crdt.author,
            Patch::Attachment(attachment) => attachment.crdt.0,
            Patch::NewAttachmentMessage(attachment) => attachment.crdt.writable.author,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SyncDataId {
    Global(i32),
    InitialSync(i32),
}
impl From<i32> for SyncDataId {
    fn from(value: i32) -> Self {
        SyncDataId::Global(value)
    }
}
impl SyncDataId {
    pub fn global(self) -> i32 {
        match self {
            SyncDataId::Global(global) => global,
            SyncDataId::InitialSync(_) => panic!("Id is not global"),
        }
    }
}
impl Default for SyncDataId {
    fn default() -> Self {
        SyncDataId::Global(Default::default())
    }
}

pub struct PatchSync<S: SyncDataSource> {
    author: Author,
    conversation: Uuid,
    ctx: S::Ctx,
    tx: VecDeque<PatchSyncMessage>,
    minimum: (i32, i32),
}
impl<S: SyncDataSource> PatchSync<S> {
    pub fn new(ctx: S::Ctx, author: Author, conversation: Uuid) -> Self {
        PatchSync {
            author,
            conversation,
            ctx,
            tx: Default::default(),
            minimum: (0, 0),
        }
    }
}
impl<S: SyncDataSource> DbSync for PatchSync<S> {
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

                let Some(next) = database.next(self.ctx, self.minimum).await? else {
                    return Ok(None);
                };

                let skip_by_conversation = next
                    .conversation()
                    .map(|conversation| conversation != self.conversation)
                    .unwrap_or(false);

                if next.author() == self.author || skip_by_conversation {
                    database.ack(self.ctx, next.id).await?;
                    continue;
                }

                match next.id {
                    SyncDataId::Global(id) => self.minimum.1 = id,
                    SyncDataId::InitialSync(id) => self.minimum.0 = id,
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
                    let valid_conversation = data
                        .conversation()
                        .map(|conversation| conversation == self.conversation)
                        .unwrap_or(true);

                    if valid_conversation {
                        if let Some(data) = database.merge(self.ctx, data).await? {
                            database.save(self.ctx, data).await?;
                        }
                    }

                    self.tx.push_back(PatchSyncMessage::Ack(id));
                }
                PatchSyncMessage::Ack(id) => database.ack(self.ctx, id).await?,
            }

            Ok(())
        }
        .boxed_local()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum PatchSyncMessage {
    Data(SyncData),
    Ack(SyncDataId),
}
impl From<SyncData> for PatchSyncMessage {
    fn from(value: SyncData) -> Self {
        PatchSyncMessage::Data(value)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use entity::{
        crdt::{sequence::CrdtWritableSequence, writable::CrdtWritable, CrdtAddOnly},
        patch::{
            Attachment, Contact, Conversation, Key, Member, MessageStatus, NewAttachmentMessage,
            NewTextMessage,
        },
    };
    use rstest::*;
    use std::collections::HashSet;

    const PEER: Author = Author(3);
    const USER: Author = Author(5);
    const PEER_PATCH: Patch = Patch::Contact(Contact {
        key: Key::zero(),
        name: String::new(),
        crdt: CrdtWritable {
            author: PEER,
            generation: 0,
        },
    });
    const USER_PATCH: Patch = Patch::Contact(Contact {
        key: Key::zero(),
        name: String::new(),
        crdt: CrdtWritable {
            author: USER,
            generation: 0,
        },
    });
    const SAME_CONVERSATION: Uuid = Uuid::from_u128(3);
    const OTHER_CONVERSATION: Uuid = Uuid::from_u128(5);

    #[derive(Clone, Default)]
    struct SourceMock {
        patches: Vec<SyncData>,
        initial_patches: Vec<SyncData>,
        minimum_ack: i32,
        merged: HashSet<SyncDataId>,
    }
    impl SyncDataSource for SourceMock {
        type Ctx = ();

        fn next(
            &mut self,
            _ctx: Self::Ctx,
            minimum: (i32, i32),
        ) -> LocalBoxFuture<DatabaseResult<Option<SyncData>>> {
            async move {
                let initial_patch = self.initial_patches.get(minimum.0 as usize).cloned();
                if let Some(initial_patch) = initial_patch {
                    return Ok(Some(initial_patch));
                }

                Ok(self
                    .patches
                    .iter()
                    .find(|patch| {
                        patch.id.global() > self.minimum_ack && patch.id.global() > minimum.1
                    })
                    .cloned())
            }
            .boxed_local()
        }

        fn ack(&mut self, _ctx: Self::Ctx, id: SyncDataId) -> LocalBoxFuture<DatabaseResult<()>> {
            async move {
                self.minimum_ack = self.minimum_ack.max(id.global());

                Ok(())
            }
            .boxed_local()
        }

        fn merge(
            &mut self,
            _ctx: Self::Ctx,
            data: SyncData,
        ) -> LocalBoxFuture<DatabaseResult<Option<SyncData>>> {
            async move {
                Ok(match self.merged.insert(data.id) {
                    true => Some(data),
                    false => None,
                })
            }
            .boxed_local()
        }

        fn save(&mut self, _ctx: Self::Ctx, data: SyncData) -> LocalBoxFuture<DatabaseResult<()>> {
            async move {
                self.patches.push(data);
                Ok(())
            }
            .boxed_local()
        }
    }

    #[rstest]
    #[case(a_contact_patch(), None)]
    #[case(a_conversation_patch(), Some(SAME_CONVERSATION))]
    #[case(a_member_patch(), Some(SAME_CONVERSATION))]
    #[case(a_text_message_patch(), Some(SAME_CONVERSATION))]
    #[case(a_message_status_patch(), Some(SAME_CONVERSATION))]
    #[case(an_attachment_patch(), Some(SAME_CONVERSATION))]
    #[case(an_attachment_message_patch(), Some(SAME_CONVERSATION))]
    fn given_a_sync_data_the_conversation_is_inferred_from_the_patch(
        #[case] patch: Patch,
        #[case] conversation: Option<Uuid>,
    ) {
        let sync_data = SyncData {
            payload: patch,
            ..Default::default()
        };

        assert_eq!(sync_data.conversation(), conversation)
    }

    #[rstest]
    #[case(a_contact_patch(), USER)]
    #[case(a_conversation_patch(), USER)]
    #[case(a_member_patch(), USER)]
    #[case(a_text_message_patch(), USER)]
    #[case(a_message_status_patch(), USER)]
    #[case(an_attachment_patch(), USER)]
    #[case(an_attachment_message_patch(), USER)]
    fn given_a_sync_data_the_author_is_inferred_from_the_patch(
        #[case] patch: Patch,
        #[case] author: Author,
    ) {
        let sync_data = SyncData {
            id: 37.into(),
            payload: patch,
        };

        assert_eq!(sync_data.author(), author);
    }

    fn a_contact_patch() -> Patch {
        Patch::Contact(Contact {
            crdt: CrdtWritable {
                author: USER,
                ..Default::default()
            },
            ..Default::default()
        })
    }
    fn a_conversation_patch() -> Patch {
        Patch::Conversation(Conversation {
            id: SAME_CONVERSATION,
            title: Default::default(),
            crdt: CrdtWritable {
                author: USER,
                ..Default::default()
            },
        })
    }
    fn a_member_patch() -> Patch {
        Patch::Member(Member {
            key: Default::default(),
            conversation: SAME_CONVERSATION,
            crdt: entity::crdt::CrdtAddOnly(USER),
        })
    }
    fn a_text_message_patch() -> Patch {
        NewTextMessage {
            id: Default::default(),
            from: Default::default(),
            conversation: SAME_CONVERSATION,
            text: Default::default(),
            crdt: CrdtWritableSequence {
                writable: CrdtWritable {
                    author: USER,
                    ..Default::default()
                },
                ..Default::default()
            },
        }
        .into()
    }
    fn a_message_status_patch() -> Patch {
        Patch::MessageStatus(MessageStatus {
            id: Default::default(),
            conversation: SAME_CONVERSATION,
            status: Default::default(),
            crdt: CrdtWritable {
                author: USER,
                ..Default::default()
            },
        })
    }
    fn an_attachment_patch() -> Patch {
        Attachment {
            id: Default::default(),
            conversation: SAME_CONVERSATION,
            payload: Default::default(),
            crdt: CrdtAddOnly(USER),
        }
        .into()
    }
    fn an_attachment_message_patch() -> Patch {
        NewAttachmentMessage {
            id: Default::default(),
            from: Default::default(),
            conversation: SAME_CONVERSATION,
            filename: Default::default(),
            attachment: Default::default(),
            crdt: CrdtWritableSequence {
                writable: CrdtWritable {
                    author: USER,
                    ..Default::default()
                },
                ..Default::default()
            },
        }
        .into()
    }

    mod given_a_patch_sync {
        use super::*;

        type Given = (SourceMock, PatchSync<SourceMock>);
        #[fixture]
        fn given() -> Given {
            let source = Default::default();
            let sync = PatchSync::new((), PEER, SAME_CONVERSATION);

            (source, sync)
        }

        #[rstest]
        #[tokio::test]
        async fn and_there_is_a_pending_patch_then_it_sends_the_patch_as_a_tx_message(
            given: Given,
        ) {
            let (mut source, mut sync, ..) = given;
            let data = SyncData {
                id: 37.into(),
                payload: USER_PATCH,
            };
            source.patches = vec![data];

            let tx = sync.tx(&mut source).await.unwrap();
            assert_eq!(tx, Some(PatchSyncMessage::Data(source.patches[0].clone())));
        }

        #[rstest]
        #[tokio::test]
        async fn and_there_are_several_pending_patches_then_it_sends_each_one_patch_then_nothing_else(
            given: Given,
        ) {
            let (mut source, mut sync, ..) = given;
            let data_initial = SyncData {
                id: SyncDataId::InitialSync(1),
                payload: USER_PATCH,
            };
            let data37 = SyncData {
                id: 37.into(),
                payload: USER_PATCH,
            };
            let data38 = SyncData {
                id: 38.into(),
                payload: USER_PATCH,
            };
            source.initial_patches = vec![data_initial];
            source.patches = vec![data37, data38];

            let tx = sync.tx(&mut source).await.unwrap();
            assert_eq!(
                tx,
                Some(PatchSyncMessage::Data(source.initial_patches[0].clone()))
            );

            let tx = sync.tx(&mut source).await.unwrap();
            assert_eq!(tx, Some(PatchSyncMessage::Data(source.patches[0].clone())));

            let tx = sync.tx(&mut source).await.unwrap();
            assert_eq!(tx, Some(PatchSyncMessage::Data(source.patches[1].clone())));

            let tx = sync.tx(&mut source).await.unwrap();
            assert_eq!(tx, None);
        }

        mod when_it_receives_a_patch {
            use super::*;

            type Given = (SourceMock, PatchSync<SourceMock>, SyncData);
            async fn given() -> Given {
                let (mut source, mut sync) = super::given();
                let data = SyncData {
                    id: 37.into(),
                    payload: PEER_PATCH,
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

            type Given = (SourceMock, PatchSync<SourceMock>, SyncData);
            async fn given() -> Given {
                let (mut source, mut sync) = super::given();
                let data = SyncData {
                    id: 37.into(),
                    payload: PEER_PATCH,
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

            let ack = PatchSyncMessage::Ack(3.into());
            sync.rx(&mut source, ack).await.unwrap();

            assert_eq!(source.minimum_ack, 3);
        }

        mod when_it_receives_a_message_with_a_different_conversation {
            use super::*;

            type Given = (SourceMock, PatchSync<SourceMock>, SyncData);
            async fn given() -> Given {
                let (mut source, mut sync, ..) = super::given();

                let diff_conv = SyncData {
                    id: 37.into(),
                    payload: Patch::Conversation(Conversation {
                        id: OTHER_CONVERSATION,
                        title: Default::default(),
                        crdt: CrdtWritable {
                            author: PEER,
                            ..Default::default()
                        },
                    }),
                };

                sync.rx(&mut source, diff_conv.clone().into())
                    .await
                    .unwrap();

                (source, sync, diff_conv)
            }

            #[rstest]
            #[tokio::test]
            async fn it_does_not_merge_the_patch() {
                let (source, ..) = given().await;

                assert_eq!(source.merged.into_iter().collect::<Vec<_>>(), vec![]);
            }

            #[rstest]
            #[tokio::test]
            async fn then_it_acknowledges_the_message() {
                let (mut source, mut sync, data, ..) = given().await;

                let tx = sync.tx(&mut source).await.unwrap();

                assert_eq!(tx, Some(PatchSyncMessage::Ack(data.id)))
            }
        }

        #[rstest]
        #[tokio::test]
        async fn when_it_receives_a_message_with_correct_conversation_it_is_merged(given: Given) {
            let (mut source, mut sync, ..) = given;

            let correct_conv = SyncData {
                id: 37.into(),
                payload: Patch::Conversation(Conversation {
                    id: SAME_CONVERSATION,
                    title: Default::default(),
                    crdt: CrdtWritable {
                        author: PEER,
                        ..Default::default()
                    },
                }),
            };

            sync.rx(&mut source, correct_conv.into()).await.unwrap();

            assert_eq!(
                source.merged.into_iter().collect::<Vec<_>>(),
                vec![SyncDataId::Global(37)]
            );
        }

        mod when_next_patch_is_from_the_peer_itself {
            use super::*;

            type Given = (SourceMock, PatchSync<SourceMock>, SyncData);
            #[fixture]
            fn given() -> Given {
                let (mut source, sync) = super::given();
                let data = SyncData {
                    id: 37.into(),
                    payload: PEER_PATCH,
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

                assert_eq!(source.minimum_ack, data.id.global());
            }

            #[rstest]
            #[tokio::test]
            async fn and_there_is_another_patch_in_the_list_then_it_sends_the_second_patch(
                given: Given,
            ) {
                let (mut source, mut sync, data, ..) = given;

                let another = SyncData {
                    id: (data.id.global() + 1).into(),
                    payload: USER_PATCH,
                };
                source.patches.push(another.clone());

                let tx = sync.tx(&mut source).await.unwrap();

                assert_eq!(tx, Some(PatchSyncMessage::Data(another)));
            }
        }

        mod when_next_patch_specifies_a_different_conversation_than_the_channels_conversation {
            use super::*;

            type Given = (SourceMock, PatchSync<SourceMock>, SyncData);
            #[fixture]
            fn given() -> Given {
                let (mut source, sync) = super::given();

                let data = SyncData {
                    id: 37.into(),
                    payload: Patch::Conversation(Conversation {
                        id: OTHER_CONVERSATION,
                        title: Default::default(),
                        crdt: CrdtWritable {
                            author: USER,
                            ..Default::default()
                        },
                    }),
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
                assert_eq!(source.minimum_ack, data.id.global())
            }

            #[rstest]
            #[tokio::test]
            async fn and_there_is_a_pending_patch_then_it_sends_the_patch_as_a_tx_message(
                given: Given,
            ) {
                let (mut source, mut sync, data, ..) = given;

                let another = SyncData {
                    id: (data.id.global() + 1).into(),
                    payload: Patch::Conversation(Conversation {
                        id: SAME_CONVERSATION,
                        title: Default::default(),
                        crdt: Default::default(),
                    }),
                };
                source.patches.push(another.clone());

                let tx = sync.tx(&mut source).await.unwrap();
                assert_eq!(tx, Some(PatchSyncMessage::Data(another)));
            }
        }
    }
}
