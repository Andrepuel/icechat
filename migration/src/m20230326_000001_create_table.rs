use crate::id::{Crdt, Id, TableConcepts};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Key::Table)
                    .col_id()
                    .col(ColumnDef::new(Key::Public).binary().not_null().unique_key())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Contact::Table)
                    .col(
                        ColumnDef::new(Contact::Key)
                            .integer()
                            .not_null()
                            .primary_key(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Contact::Table, Contact::Key)
                            .to(Key::Table, Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(Contact::Name).text().not_null())
                    .crdt_writable()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Local::Table)
                    .col(
                        ColumnDef::new(Local::Key)
                            .integer()
                            .not_null()
                            .primary_key(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Local::Table, Local::Key)
                            .to(Key::Table, Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .col(ColumnDef::new(Local::Private).binary().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Conversation::Table)
                    .col_id()
                    .col_uuid()
                    .col(ColumnDef::new(Conversation::Title).text())
                    .crdt_writable()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Member::Table)
                    .col(ColumnDef::new(Member::Contact).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Member::Table, Member::Contact)
                            .to(Contact::Table, Contact::Key)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(Member::Table, Member::Contact)
                            .to(Key::Table, Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(Member::Conversation).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Member::Table, Member::Conversation)
                            .to(Conversation::Table, Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .primary_key(
                        Index::create()
                            .col(Member::Contact)
                            .col(Member::Conversation),
                    )
                    .crdt_add_only()
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
                    .crdt_writable()
                    .alternative_crdt_writable(
                        Message::StatusCrdtGeneration,
                        Message::StatusCrdtAuthor,
                    )
                    .crdt_sequence()
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

        manager
            .create_table(
                Table::create()
                    .table(Channel::Table)
                    .col_id()
                    .col(ColumnDef::new(Channel::Conversation).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Channel::Table, Channel::Conversation)
                            .to(Conversation::Table, Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(Channel::Peer).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(Channel::Table, Channel::Peer)
                            .to(Key::Table, Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .col(ColumnDef::new(Channel::SyncIndex).integer().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Sync::Table)
                    .col_id()
                    .col(ColumnDef::new(Sync::Payload).binary().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(InitialSync::Table)
                    .col_id()
                    .col(ColumnDef::new(InitialSync::Channel).integer().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .from(InitialSync::Table, InitialSync::Channel)
                            .to(Channel::Table, Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .col(ColumnDef::new(InitialSync::Payload).binary().not_null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(InitialSync::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Sync::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Channel::Table).to_owned())
            .await?;

        manager
            .drop_index(Index::drop().name("message_crdt_sequence_index").to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Message::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Member::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Conversation::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Local::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Contact::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Key::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(Iden)]
pub enum Key {
    Table,
    Public,
}

#[derive(Iden)]
pub enum Contact {
    Table,
    Key,
    Name,
}

#[derive(Iden)]
enum Local {
    Table,
    Key,
    Private,
}

#[derive(Iden)]
pub enum Conversation {
    Table,
    Title,
}

#[derive(Iden)]
enum Member {
    Table,
    Contact,
    Conversation,
}

#[derive(Iden)]
pub enum Message {
    Table,
    Status,
    From,
    Conversation,
    Text,
    StatusCrdtGeneration,
    StatusCrdtAuthor,
    Attachment,
}

#[derive(Iden)]
enum Channel {
    Table,
    Conversation,
    Peer,
    SyncIndex,
}

#[derive(Iden)]
enum Sync {
    Table,
    Payload,
}

#[derive(Iden)]
enum InitialSync {
    Table,
    Channel,
    Payload,
}
