use crate::entity::{conversation, message};
use sea_orm::{sea_query::IntoCondition, ColumnTrait};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SplitUuid(pub i32, pub i32, pub i32, pub i32);
impl From<Uuid> for SplitUuid {
    fn from(value: Uuid) -> Self {
        let value = value.as_bytes();
        let value0 = i32::from_be_bytes(value[0..][..4].try_into().unwrap());
        let value1 = i32::from_be_bytes(value[4..][..4].try_into().unwrap());
        let value2 = i32::from_be_bytes(value[8..][..4].try_into().unwrap());
        let value3 = i32::from_be_bytes(value[12..][..4].try_into().unwrap());

        Self(value0, value1, value2, value3)
    }
}
impl From<SplitUuid> for Uuid {
    fn from(split: SplitUuid) -> Self {
        let mut value = [0; 16];
        value[0..][..4].copy_from_slice(&split.0.to_be_bytes());
        value[4..][..4].copy_from_slice(&split.1.to_be_bytes());
        value[8..][..4].copy_from_slice(&split.2.to_be_bytes());
        value[12..][..4].copy_from_slice(&split.3.to_be_bytes());
        Uuid::from_bytes(value)
    }
}
impl SplitUuid {
    pub fn to_filter<T: UuidColumn>(
        &self,
    ) -> (
        impl IntoCondition,
        impl IntoCondition,
        impl IntoCondition,
        impl IntoCondition,
    ) {
        let cols = T::get_column();

        (
            cols[0].eq(self.0),
            cols[1].eq(self.1),
            cols[2].eq(self.2),
            cols[3].eq(self.3),
        )
    }
}

pub trait UuidColumn: ColumnTrait + Sized {
    fn get_column() -> [Self; 4];
}

pub trait UuidValue {
    fn get_uuid(&self) -> SplitUuid;
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn conversion_test() {
        let uuid = uuid::uuid!("f533992a-d13b-4b22-9b7e-9b62efcca545");

        let split = SplitUuid::from(uuid);
        assert_eq!(
            split,
            SplitUuid(
                0xf533992a_u32 as i32,
                0xd13b4b22_u32 as i32,
                0x9b7e9b62_u32 as i32,
                0xefcca545_u32 as i32
            )
        );

        let uuid_back = Uuid::from(split);
        assert_eq!(uuid_back, uuid);
    }
}

impl UuidColumn for conversation::Column {
    fn get_column() -> [Self; 4] {
        [
            conversation::Column::Uuid0,
            conversation::Column::Uuid1,
            conversation::Column::Uuid2,
            conversation::Column::Uuid3,
        ]
    }
}
impl UuidValue for conversation::Model {
    fn get_uuid(&self) -> SplitUuid {
        SplitUuid(
            self.uuid0,
            self.uuid1,
            self.uuid2,
            self.uuid3,
        )
    }
}

impl UuidColumn for message::Column {
    fn get_column() -> [Self; 4] {
        [
            message::Column::Uuid0,
            message::Column::Uuid1,
            message::Column::Uuid2,
            message::Column::Uuid3,
        ]
    }
}
impl UuidValue for message::Model {
    fn get_uuid(&self) -> SplitUuid {
        SplitUuid(
            self.uuid0,
            self.uuid1,
            self.uuid2,
            self.uuid3,
        )
    }
}
