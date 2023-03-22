use automerge::{sync::ReadMessageError, AutomergeError, ObjType};
use sea_orm::DbErr;

#[derive(thiserror::Error, Debug)]
pub enum DatabaseError {
    #[error(transparent)]
    DbErr(#[from] DbErr),
    #[error(transparent)]
    AutomergeError(#[from] AutomergeError),
    #[error(transparent)]
    UnexpectedType(#[from] UnexpectedType),
    #[error("Bytes does not represent a raw UUID")]
    BadUuid,
    #[error(transparent)]
    BadSyncMessage(#[from] ReadMessageError),
}
pub type DatabaseResult<T> = Result<T, DatabaseError>;

#[derive(thiserror::Error, Debug)]
#[error("Data is {actual:?} type, but type {expected:?} is expected")]
pub struct UnexpectedType {
    actual: DataType,
    expected: DataType,
}
impl From<(DataType, DataType)> for UnexpectedType {
    fn from((actual, expected): (DataType, DataType)) -> Self {
        UnexpectedType { actual, expected }
    }
}
impl UnexpectedType {
    pub fn from_actual(actual: automerge::Value<'_>, expected: DataType) -> Self {
        UnexpectedType {
            actual: actual.into(),
            expected,
        }
    }

    pub fn from_actual_scalar(actual: &automerge::ScalarValue, expected: DataType) -> Self {
        UnexpectedType {
            actual: actual.into(),
            expected,
        }
    }

    pub fn from_nil(expected: DataType) -> Self {
        UnexpectedType {
            actual: DataType::Null,
            expected,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub enum DataType {
    Map,
    Table,
    List,
    Text,
    Bytes,
    Str,
    Int,
    Uint,
    F64,
    Counter,
    Timestamp,
    Boolean,
    Unknown,
    #[default]
    Null,
    AnyScalar,
}
impl<'a> From<automerge::Value<'a>> for DataType {
    fn from(value: automerge::Value<'a>) -> Self {
        match value {
            automerge::Value::Object(ObjType::Map) => Self::Map,
            automerge::Value::Object(ObjType::Table) => Self::Table,
            automerge::Value::Object(ObjType::List) => Self::List,
            automerge::Value::Object(ObjType::Text) => Self::Text,
            automerge::Value::Scalar(scalar) => scalar.as_ref().into(),
        }
    }
}
impl From<&automerge::ScalarValue> for DataType {
    fn from(value: &automerge::ScalarValue) -> Self {
        match value {
            automerge::ScalarValue::Bytes(_) => Self::Bytes,
            automerge::ScalarValue::Str(_) => Self::Str,
            automerge::ScalarValue::Int(_) => Self::Int,
            automerge::ScalarValue::Uint(_) => Self::Uint,
            automerge::ScalarValue::F64(_) => Self::F64,
            automerge::ScalarValue::Counter(_) => Self::Counter,
            automerge::ScalarValue::Timestamp(_) => Self::Timestamp,
            automerge::ScalarValue::Boolean(_) => Self::Boolean,
            automerge::ScalarValue::Unknown { .. } => Self::Unknown,
            automerge::ScalarValue::Null => Self::Null,
        }
    }
}
