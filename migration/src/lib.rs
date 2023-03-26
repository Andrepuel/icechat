pub use sea_orm_migration::prelude::*;

pub mod id;
mod m20230326_000001_add_attachment;
mod m20230326_000001_create_table;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20230326_000001_create_table::Migration),
            Box::new(m20230326_000001_add_attachment::Migration),
        ]
    }
}
