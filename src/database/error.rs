use sea_orm::DbErr;

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error(transparent)]
    DbErr(#[from] DbErr),
}
impl From<sqlx::error::Error> for DatabaseError {
    fn from(value: sqlx::error::Error) -> Self {
        DbErr::Conn(sea_orm::RuntimeErr::SqlxError(value)).into()
    }
}
pub type DatabaseResult<T> = Result<T, DatabaseError>;
