use sea_orm_migration::prelude::*;

#[async_std::main]
async fn main() {
    env_logger::init();
    cli::run_cli(migration::Migrator).await;
}
