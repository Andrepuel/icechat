use super::{attachment::AttachmentMetaModel, Key};
use crate::{
    crdt::{sequence::CrdtWritableSequence, writable::CrdtWritable, Author},
    entity::{conversation, key, message},
    uuid::UuidValue,
};
use either::Either;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NewAttachmentMessage {
    pub id: Uuid,
    pub from: Key,
    pub conversation: Uuid,
    pub filename: String,
    pub attachment: Uuid,
    pub crdt: CrdtWritableSequence,
}
impl NewAttachmentMessage {
    pub fn into_crdt(self) -> NewMessage {
        NewMessage {
            id: self.id,
            from: self.from,
            conversation: self.conversation,
            text: self.filename,
            attachment: Some(self.attachment),
            crdt: self.crdt,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct NewTextMessage {
    pub id: Uuid,
    pub from: Key,
    pub conversation: Uuid,
    pub text: String,
    pub crdt: CrdtWritableSequence,
}
impl NewTextMessage {
    pub fn into_crdt(self) -> NewMessage {
        NewMessage {
            id: self.id,
            from: self.from,
            conversation: self.conversation,
            text: self.text,
            attachment: None,
            crdt: self.crdt,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NewMessage {
    pub id: Uuid,
    pub from: Key,
    pub conversation: Uuid,
    pub text: String,
    pub attachment: Option<Uuid>,
    pub crdt: CrdtWritableSequence,
}
impl
    From<(
        message::Model,
        key::Model,
        Uuid,
        Option<AttachmentMetaModel>,
    )> for NewMessage
{
    fn from(
        (message, from, conversation, attachment): (
            message::Model,
            key::Model,
            Uuid,
            Option<AttachmentMetaModel>,
        ),
    ) -> Self {
        let id = message.get_uuid();
        let from = Key::new(from.public).expect("Inconsistent database");
        let attachment = attachment.map(|attachment| attachment.get_uuid().into());

        NewMessage {
            id: id.into(),
            from,
            conversation,
            text: message.text,
            attachment,
            crdt: CrdtWritableSequence {
                writable: CrdtWritable {
                    author: Author(message.crdt_author),
                    generation: message.crdt_generation,
                },
                sequence: message.crdt_sequence,
            },
        }
    }
}
impl
    From<(
        message::Model,
        key::Model,
        conversation::Model,
        Option<AttachmentMetaModel>,
    )> for NewMessage
{
    fn from(
        (message, from, conversation, attachment): (
            message::Model,
            key::Model,
            conversation::Model,
            Option<AttachmentMetaModel>,
        ),
    ) -> Self {
        let conversation = Uuid::from(conversation.get_uuid());

        (message, from, conversation, attachment).into()
    }
}
impl NewMessage {
    pub fn into_serializable(self) -> Either<NewTextMessage, NewAttachmentMessage> {
        match self.attachment {
            Some(attachment) => Either::Right(NewAttachmentMessage {
                id: self.id,
                from: self.from,
                conversation: self.conversation,
                filename: self.text,
                attachment,
                crdt: self.crdt,
            }),
            None => Either::Left(NewTextMessage {
                id: self.id,
                from: self.from,
                conversation: self.conversation,
                text: self.text,
                crdt: self.crdt,
            }),
        }
    }

    pub fn into_text(self) -> NewTextMessage {
        match self.into_serializable() {
            Either::Left(text) => text,
            Either::Right(_) => panic!(),
        }
    }

    pub fn into_attachment(self) -> NewAttachmentMessage {
        match self.into_serializable() {
            Either::Left(_) => panic!(),
            Either::Right(attachment) => attachment,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageStatus {
    pub id: Uuid,
    pub conversation: Uuid,
    pub status: i32,
    pub crdt: CrdtWritable,
}
impl From<(message::Model, Uuid)> for MessageStatus {
    fn from((message, conversation): (message::Model, Uuid)) -> Self {
        let id = message.get_uuid();

        MessageStatus {
            id: id.into(),
            conversation,
            status: message.status,
            crdt: CrdtWritable {
                author: Author(message.status_crdt_author),
                generation: message.status_crdt_generation,
            },
        }
    }
}
impl From<(message::Model, conversation::Model)> for MessageStatus {
    fn from((message, conversation): (message::Model, conversation::Model)) -> Self {
        let conversation = Uuid::from(conversation.get_uuid());

        (message, conversation).into()
    }
}
