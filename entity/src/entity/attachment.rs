//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.0

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "attachment")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub uuid0: i32,
    pub uuid1: i32,
    pub uuid2: i32,
    pub uuid3: i32,
    pub conversation: i32,
    pub payload: Option<Vec<u8>>,
    pub crdt_author: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::conversation::Entity",
        from = "Column::Conversation",
        to = "super::conversation::Column::Id",
        on_update = "NoAction",
        on_delete = "NoAction"
    )]
    Conversation,
    #[sea_orm(has_many = "super::message::Entity")]
    Message,
}

impl Related<super::conversation::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Conversation.def()
    }
}

impl Related<super::message::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Message.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}