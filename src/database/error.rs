use sea_orm::DbErr;

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error(transparent)]
    DbErr(#[from] DbErr),
}
pub type DatabaseResult<T> = Result<T, DatabaseError>;
