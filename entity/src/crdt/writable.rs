use super::{Author, CrdtInstance, CrdtOrd, CrdtTransaction};
use futures::{future::LocalBoxFuture, FutureExt};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrdtWritable {
    pub generation: i32,
    pub author: Author,
}
impl CrdtOrd for CrdtWritable {
    fn next(&self, author: Author) -> Self {
        CrdtWritable {
            author,
            generation: self.generation + 1,
        }
    }
}

impl<V: CrdtInstance<Crdt = CrdtWritable> + 'static, S: CrdtTransaction<V>>
    CrdtWritableTransaction<V> for S
{
}
pub trait CrdtWritableTransaction<V: CrdtInstance<Crdt = CrdtWritable> + 'static>:
    CrdtTransaction<V>
{
    fn add(&mut self, author: Author, mut value: V) -> LocalBoxFuture<'_, V> {
        async move {
            let existent = self.existent(value.id()).await;
            let existent_crdt = existent
                .as_ref()
                .map(|(_, existent)| existent.crdt())
                .unwrap_or_default();

            value.set_crdt(existent_crdt.next(author));

            self.save(value, existent).await
        }
        .boxed_local()
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct CrdtValueMock(usize, i32, i32);
    impl CrdtInstance for CrdtValueMock {
        type Id = usize;
        type Crdt = CrdtWritable;

        fn id(&self) -> Self::Id {
            0
        }

        fn crdt(&self) -> Self::Crdt {
            CrdtWritable {
                author: Author(self.2),
                generation: self.1,
            }
        }

        fn set_crdt(&mut self, crdt: CrdtWritable) {
            self.1 = crdt.generation;
            self.2 = crdt.author.0;
        }
    }

    struct CrdtValueTransactionMock<T: Clone + CrdtInstance>(Vec<T>);
    impl<T: Clone + CrdtInstance> Default for CrdtValueTransactionMock<T> {
        fn default() -> Self {
            Self(Default::default())
        }
    }
    impl<V: Clone + CrdtInstance + 'static> CrdtTransaction<V> for CrdtValueTransactionMock<V>
    where
        V::Id: Eq,
    {
        type RowId = usize;

        fn save(&mut self, value: V, existent: Option<(usize, V)>) -> LocalBoxFuture<'_, V> {
            async move {
                match existent {
                    Some((idx, _)) => self.0[idx] = value.clone(),
                    None => self.0.push(value.clone()),
                }

                value
            }
            .boxed_local()
        }

        fn existent(&mut self, id: V::Id) -> LocalBoxFuture<'_, Option<(usize, V)>> {
            async move {
                self.0
                    .iter_mut()
                    .enumerate()
                    .find(|(_, x)| x.id() == id)
                    .map(|(idx, v)| (idx, v.clone()))
            }
            .boxed_local()
        }
    }

    mod when_adding_a_new_value {
        use super::*;

        #[tokio::test]
        async fn on_empty_list_the_value_is_added_with_gen_1() {
            let mut list = CrdtValueTransactionMock::default();

            let new_value = CrdtValueMock(0, 0, 0);
            let inserted = list.add(Author(5), new_value).await;

            assert_eq!(inserted, CrdtValueMock(0, 1, 5));
            assert_eq!(list.0, [CrdtValueMock(0, 1, 5)]);
        }

        #[tokio::test]
        async fn on_replacing_value_the_gen_will_be_the_next_in_sequence() {
            let mut list = CrdtValueTransactionMock(vec![CrdtValueMock(0, 2, 3)]);

            let new_value = CrdtValueMock(0, 0, 0);
            let inserted = list.add(Author(5), new_value).await;

            assert_eq!(inserted, CrdtValueMock(0, 3, 5));
            assert_eq!(list.0, [CrdtValueMock(0, 3, 5)]);
        }
    }

    mod when_merging_a_value {
        use super::*;

        #[tokio::test]
        async fn on_empty_list_the_value_is_added_and_returned() {
            let mut list = CrdtValueTransactionMock::default();

            let new_value = CrdtValueMock(0, 3, 5);
            let inserted = list.merge(new_value).await;

            assert_eq!(inserted, Some(new_value));
            assert_eq!(list.0, [new_value]);
        }

        #[tokio::test]
        async fn on_merging_older_value_nothing_is_done() {
            let newer_value = CrdtValueMock(0, 4, 7);
            let mut list = CrdtValueTransactionMock(vec![newer_value]);

            let new_value = CrdtValueMock(0, 3, 5);
            let inserted = list.merge(new_value).await;

            assert_eq!(inserted, None);
            assert_eq!(list.0, [newer_value]);
        }

        #[tokio::test]
        async fn on_merging_newer_value_it_is_returned_and_added() {
            let older_value = CrdtValueMock(0, 2, 7);
            let mut list = CrdtValueTransactionMock(vec![older_value]);

            let new_value = CrdtValueMock(0, 3, 5);
            let inserted = list.merge(new_value).await;

            assert_eq!(inserted, Some(new_value));
            assert_eq!(list.0, [new_value]);
        }
    }
}
