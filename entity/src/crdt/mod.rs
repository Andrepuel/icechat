pub mod contact;
pub mod conversation;
pub mod member;
pub mod message;
pub mod sequence;
pub mod writable;

use futures::{future::LocalBoxFuture, FutureExt};
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct Author(pub i32);

pub trait CrdtInstance: Sized {
    type Id;
    type Crdt: CrdtOrd;

    fn id(&self) -> Self::Id;
    fn crdt(&self) -> Self::Crdt;
    fn set_crdt(&mut self, crdt: Self::Crdt);
}

pub trait CrdtOrd: Ord + Default + Sized {
    fn next(&self, author: Author) -> Self;
}

#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrdtAddOnly;
impl CrdtOrd for CrdtAddOnly {
    fn next(&self, _author: Author) -> Self {
        CrdtAddOnly
    }
}

pub trait CrdtTransaction<V: CrdtInstance + 'static> {
    type RowId;

    fn merge(&mut self, value: V) -> LocalBoxFuture<'_, Option<V>> {
        async move {
            let existent_rowid = self.existent(value.id()).await;
            let existent = existent_rowid.as_ref().map(|(_, existent)| existent);

            if let Some(existent) = existent {
                if existent.crdt() > value.crdt() {
                    return None;
                }
            }

            Some(self.save(value, existent_rowid).await)
        }
        .boxed_local()
    }

    fn save(&mut self, value: V, existent: Option<(Self::RowId, V)>) -> LocalBoxFuture<'_, V>;
    fn existent(
        &mut self,
        id: <V as CrdtInstance>::Id,
    ) -> LocalBoxFuture<'_, Option<(Self::RowId, V)>>;
}
