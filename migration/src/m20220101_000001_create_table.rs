use sea_orm_migration::prelude::*;
use std::borrow::BorrowMut;

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
struct Id;

#[allow(clippy::enum_variant_names)]
#[derive(Iden)]
enum Crdt {
    CrdtGeneration,
    CrdtAuthor,
    CrdtSequence,
}

#[derive(Iden)]
enum Uuid {
    Uuid0,
    Uuid1,
    Uuid2,
    Uuid3,
}

impl<S: BorrowMut<TableCreateStatement>> TableConcepts for S {}
trait TableConcepts: BorrowMut<TableCreateStatement> {
    fn typed(&mut self) -> &mut TableCreateStatement {
        self.borrow_mut()
    }

    fn col_id(&mut self) -> &mut TableCreateStatement {
        self.typed().col(
            ColumnDef::new(Id)
                .integer()
                .not_null()
                .auto_increment()
                .primary_key(),
        )
    }

    fn col_uuid(&mut self) -> &mut TableCreateStatement {
        self.typed()
            .col(ColumnDef::new(Uuid::Uuid0).integer().not_null())
            .col(ColumnDef::new(Uuid::Uuid1).integer().not_null())
            .col(ColumnDef::new(Uuid::Uuid2).integer().not_null())
            .col(ColumnDef::new(Uuid::Uuid3).integer().not_null())
            .index(
                Index::create()
                    .unique()
                    .name("uuid")
                    .col(Uuid::Uuid0)
                    .col(Uuid::Uuid1)
                    .col(Uuid::Uuid2)
                    .col(Uuid::Uuid3),
            )
    }

    fn crdt_writable(&mut self) -> &mut TableCreateStatement {
        self.typed()
            .col(ColumnDef::new(Crdt::CrdtGeneration).integer().not_null())
            .col(ColumnDef::new(Crdt::CrdtAuthor).integer().not_null())
    }

    fn crdt_sequence(&mut self) -> &mut TableCreateStatement {
        self.typed()
            .col(ColumnDef::new(Crdt::CrdtSequence).integer().not_null())
    }

    fn crdt_add_only(&mut self) -> &mut TableCreateStatement {
        self.typed()
            .col(ColumnDef::new(Crdt::CrdtAuthor).integer().not_null())
    }
}

#[derive(Iden)]
enum Key {
    Table,
    Public,
}

#[derive(Iden)]
enum Contact {
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
enum Conversation {
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
enum Message {
    Table,
    Status,
    From,
    Conversation,
    Text,
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
