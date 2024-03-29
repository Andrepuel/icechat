//! `SeaORM` Entity. Generated by sea-orm-codegen 0.11.0

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "local")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub key: i32,
    pub private: Vec<u8>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::key::Entity",
        from = "Column::Key",
        to = "super::key::Column::Id",
        on_update = "NoAction",
        on_delete = "Restrict"
    )]
    Key,
}

impl Related<super::key::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Key.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
