use sea_orm_migration::prelude::*;
use std::borrow::BorrowMut;

#[derive(Iden)]
pub struct Id;

#[allow(clippy::enum_variant_names)]
#[derive(Iden)]
pub enum Crdt {
    CrdtGeneration,
    CrdtAuthor,
    CrdtSequence,
}

#[derive(Iden)]
pub enum Uuid {
    Uuid0,
    Uuid1,
    Uuid2,
    Uuid3,
}

impl<S: BorrowMut<TableCreateStatement>> TableConcepts for S {}
pub trait TableConcepts: BorrowMut<TableCreateStatement> {
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
        let gen = Crdt::CrdtGeneration;
        let author = Crdt::CrdtAuthor;
        self.alternative_crdt_writable(gen, author)
    }

    fn alternative_crdt_writable<T: IntoIden>(
        &mut self,
        gen: T,
        author: T,
    ) -> &mut TableCreateStatement {
        self.typed()
            .col(ColumnDef::new(gen).integer().not_null())
            .col(ColumnDef::new(author).integer().not_null())
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
