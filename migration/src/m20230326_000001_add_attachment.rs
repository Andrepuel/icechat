use crate::{
    id::{Crdt, Id, TableConcepts},
    m20230326_000001_create_table::{Contact, Conversation, Key, Message},
};
use sea_orm::{ActiveModelTrait, ActiveValue};
use sea_orm_migration::{prelude::*, sea_orm::EntityTrait};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Attachment::Table)
                    .col_id()
                    .col_uuid()
                    .col(
                        ColumnDef::new(Attachment::Conversation)
                            .integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Attachment::Table, Attachment::Conversation)
                            .to(Conversation::Table, Id),
                    )
                    .col(ColumnDef::new(Attachment::Payload).binary())
                    .crdt_add_only()
                    .to_owned(),
            )
            .await?;

        manager
            .drop_index(
                Index::drop()
                    .name("message_crdt_sequence_index")
                    .table(Message::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .rename_table(
                Table::rename()
                    .table(Message::Table, OldMessage::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Message::Table)
                    .col_id()
                    .col_uuid()
                    .col(ColumnDef::new(Message::Status).integer().not_null())
                    .col(ColumnDef::new(Message::From).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Message::Table, Message::From)
                            .to(Contact::Table, Contact::Key)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Message::Table, Message::From)
                            .to(Key::Table, Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .col(ColumnDef::new(Message::Conversation).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Message::Table, Message::Conversation)
                            .to(Conversation::Table, Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(Message::Text).text().not_null())
                    .col(ColumnDef::new(Message::Attachment).integer())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Message::Table, Message::Attachment)
                            .to(Attachment::Table, Id),
                    )
                    .crdt_writable()
                    .alternative_crdt_writable(
                        Message::StatusCrdtGeneration,
                        Message::StatusCrdtAuthor,
                    )
                    .crdt_sequence()
                    .to_owned(),
            )
            .await?;

        let conn = manager.get_connection();
        let old_messages = old_message::Entity::find().all(conn).await?;
        for message in old_messages {
            entity::entity::message::ActiveModel {
                id: ActiveValue::Set(message.id),
                uuid0: ActiveValue::Set(message.uuid0),
                uuid1: ActiveValue::Set(message.uuid1),
                uuid2: ActiveValue::Set(message.uuid2),
                uuid3: ActiveValue::Set(message.uuid3),
                status: ActiveValue::Set(message.status),
                from: ActiveValue::Set(message.from),
                conversation: ActiveValue::Set(message.conversation),
                text: ActiveValue::Set(message.text),
                attachment: ActiveValue::Set(None),
                crdt_generation: ActiveValue::Set(message.crdt_generation),
                crdt_author: ActiveValue::Set(message.crdt_author),
                status_crdt_generation: ActiveValue::Set(message.status_crdt_generation),
                status_crdt_author: ActiveValue::Set(message.status_crdt_author),
                crdt_sequence: ActiveValue::Set(message.crdt_sequence),
            }
            .insert(conn)
            .await?;
        }

        manager
            .create_index(
                Index::create()
                    .name("message_crdt_sequence_index")
                    .table(Message::Table)
                    .col(Crdt::CrdtSequence)
                    .to_owned(),
            )
            .await?;

        manager
            .drop_table(Table::drop().table(OldMessage::Table).to_owned())
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("message_crdt_sequence_index")
                    .table(Message::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(OldMessage::Table)
                    .col_id()
                    .col_uuid()
                    .col(ColumnDef::new(Message::Status).integer().not_null())
                    .col(ColumnDef::new(Message::From).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(OldMessage::Table, Message::From)
                            .to(Contact::Table, Contact::Key)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(OldMessage::Table, Message::From)
                            .to(Key::Table, Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .col(ColumnDef::new(Message::Conversation).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(OldMessage::Table, Message::Conversation)
                            .to(Conversation::Table, Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(Message::Text).text().not_null())
                    .crdt_writable()
                    .alternative_crdt_writable(
                        Message::StatusCrdtGeneration,
                        Message::StatusCrdtAuthor,
                    )
                    .crdt_sequence()
                    .to_owned(),
            )
            .await?;

        let conn = manager.get_connection();
        let messages = entity::entity::message::Entity::find().all(conn).await?;
        for message in messages {
            old_message::ActiveModel {
                id: ActiveValue::Set(message.id),
                uuid0: ActiveValue::Set(message.uuid0),
                uuid1: ActiveValue::Set(message.uuid1),
                uuid2: ActiveValue::Set(message.uuid2),
                uuid3: ActiveValue::Set(message.uuid3),
                status: ActiveValue::Set(message.status),
                from: ActiveValue::Set(message.from),
                conversation: ActiveValue::Set(message.conversation),
                text: ActiveValue::Set(message.text),
                crdt_generation: ActiveValue::Set(message.crdt_generation),
                crdt_author: ActiveValue::Set(message.crdt_author),
                status_crdt_generation: ActiveValue::Set(message.status_crdt_generation),
                status_crdt_author: ActiveValue::Set(message.status_crdt_author),
                crdt_sequence: ActiveValue::Set(message.crdt_sequence),
            }
            .insert(conn)
            .await?;
        }

        manager
            .drop_table(Table::drop().table(Message::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Attachment::Table).to_owned())
            .await?;

        manager
            .rename_table(
                Table::rename()
                    .table(OldMessage::Table, Message::Table)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("message_crdt_sequence_index")
                    .table(Message::Table)
                    .col(Crdt::CrdtSequence)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

/// Learn more at https://docs.rs/sea-query#iden
#[derive(Iden)]
enum Attachment {
    Table,
    Conversation,
    Payload,
}

#[derive(Iden)]
enum OldMessage {
    Table,
}

mod old_message {
    use sea_orm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
    #[sea_orm(table_name = "old_message")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub uuid0: i32,
        pub uuid1: i32,
        pub uuid2: i32,
        pub uuid3: i32,
        pub status: i32,
        pub from: i32,
        pub conversation: i32,
        pub text: String,
        pub crdt_generation: i32,
        pub crdt_author: i32,
        pub status_crdt_generation: i32,
        pub status_crdt_author: i32,
        pub crdt_sequence: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
