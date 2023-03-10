pub mod contact;
pub mod conversation;
pub mod member;
pub mod message;

use futures::{future::LocalBoxFuture, FutureExt};
use serde::{Deserialize, Serialize};

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
pub struct Author(pub i32);

pub trait CrdtValue: Sized {
    type Id;
    type Crdt: CrdtOrd;

    fn id(&self) -> Self::Id;
    fn crdt(&self) -> Self::Crdt;
    fn set_crdt(&mut self, crdt: Self::Crdt);

    fn merge(self, existent: Option<&Self>) -> Option<Self> {
        let Some(existent) = existent else { return Some(self) };

        if self.crdt() > existent.crdt() {
            Some(self)
        } else {
            None
        }
    }
}

pub trait CrdtOrd: Ord + Default + Sized {
    fn next(&self, author: Author) -> Self;
}

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

#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrdtWritableSequence {
    pub writable: CrdtWritable,
    pub sequence: i32,
}
impl CrdtOrd for CrdtWritableSequence {
    fn next(&self, author: Author) -> Self {
        CrdtWritableSequence {
            writable: self.writable.next(author),
            sequence: self.sequence,
        }
    }
}

#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrdtAddOnly;
impl CrdtOrd for CrdtAddOnly {
    fn next(&self, _author: Author) -> Self {
        CrdtAddOnly
    }
}

pub trait CrdtValueTransaction<V: CrdtValue + 'static> {
    type RowId;

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

    fn merge(&mut self, value: V) -> LocalBoxFuture<'_, Option<V>> {
        async move {
            let existent_rowid = self.existent(value.id()).await;
            let existent = existent_rowid.as_ref().map(|(_, existent)| existent);
            let value = value.merge(existent)?;

            Some(self.save(value, existent_rowid).await)
        }
        .boxed_local()
    }

    fn save(&mut self, value: V, existent: Option<(Self::RowId, V)>) -> LocalBoxFuture<'_, V>;
    fn existent(
        &mut self,
        id: <V as CrdtValue>::Id,
    ) -> LocalBoxFuture<'_, Option<(Self::RowId, V)>>;
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct CrdtValueMock(usize, i32, i32);
    impl CrdtValue for CrdtValueMock {
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

    struct CrdtValueTransactionMock<T: Clone + CrdtValue>(Vec<T>);
    impl<T: Clone + CrdtValue> Default for CrdtValueTransactionMock<T> {
        fn default() -> Self {
            Self(Default::default())
        }
    }
    impl<V: Clone + CrdtValue + 'static> CrdtValueTransaction<V> for CrdtValueTransactionMock<V>
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

    mod when_merging {
        use super::*;

        #[test]
        fn if_there_is_nothing_already_existing_then_merge_overwrites() {
            let my_value = CrdtValueMock(0, 0, 0);

            assert_eq!(my_value.merge(None), Some(my_value));
        }

        #[test]
        fn if_the_element_is_same_nothing_is_done() {
            let my_value = CrdtValueMock(0, 0, 0);

            assert_eq!(my_value.merge(Some(&my_value)), None);
        }

        #[test]
        fn the_bigger_generation_will_wins_the_merge() {
            let bigger_gen = CrdtValueMock(0, 10, 1);
            let smaller_gen = CrdtValueMock(0, 9, 100);

            assert_eq!(bigger_gen.merge(Some(&smaller_gen)), Some(bigger_gen));
            assert_eq!(smaller_gen.merge(Some(&bigger_gen)), None);
        }

        #[test]
        fn if_the_generation_is_same_the_bigger_author_wins_the_merge() {
            let bigger_author = CrdtValueMock(0, 0, 100);
            let smaller_author = CrdtValueMock(0, 0, 10);

            assert_eq!(
                bigger_author.merge(Some(&smaller_author)),
                Some(bigger_author)
            );
            assert_eq!(smaller_author.merge(Some(&bigger_author)), None);
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
