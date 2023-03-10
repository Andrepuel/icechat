use super::{writable::CrdtWritable, Author, CrdtInstance, CrdtOrd, CrdtTransaction};
use futures::{future::LocalBoxFuture, FutureExt};
use serde::{Deserialize, Serialize};

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

pub trait CrdtWritableSequenceTransaction<V: CrdtInstance<Crdt = CrdtWritableSequence> + 'static>:
    CrdtTransaction<V>
{
    fn push(&mut self, author: Author, mut value: V) -> LocalBoxFuture<'_, V> {
        async move {
            let existent = self.existent(value.id()).await;
            let mut crdt = existent
                .as_ref()
                .map(|(_, existent)| existent.crdt())
                .unwrap_or_default()
                .next(author);
            crdt.sequence = self.last().await.unwrap_or_default().sequence + 1;
            value.set_crdt(crdt);

            self.save(value, existent).await
        }
        .boxed_local()
    }

    fn last(&mut self) -> LocalBoxFuture<'_, Option<CrdtWritableSequence>>;
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct CrdtSequenceMock(char, i32, i32, i32);
    impl CrdtInstance for CrdtSequenceMock {
        type Id = char;
        type Crdt = CrdtWritableSequence;

        fn id(&self) -> Self::Id {
            self.0
        }

        fn crdt(&self) -> Self::Crdt {
            CrdtWritableSequence {
                writable: CrdtWritable {
                    generation: self.1,
                    author: Author(self.2),
                },
                sequence: self.3,
            }
        }

        fn set_crdt(&mut self, crdt: Self::Crdt) {
            self.1 = crdt.writable.generation;
            self.2 = crdt.writable.author.0;
            self.3 = crdt.sequence;
        }
    }

    #[derive(Default)]
    struct CrdtWritableSequenceTransactionMock(Vec<CrdtSequenceMock>);
    impl CrdtTransaction<CrdtSequenceMock> for CrdtWritableSequenceTransactionMock {
        type RowId = usize;

        fn save(
            &mut self,
            value: CrdtSequenceMock,
            existent: Option<(Self::RowId, CrdtSequenceMock)>,
        ) -> LocalBoxFuture<'_, CrdtSequenceMock> {
            async move {
                match existent {
                    Some((row, _)) => self.0[row] = value,
                    None => self.0.push(value),
                }

                value
            }
            .boxed_local()
        }

        fn existent(
            &mut self,
            id: char,
        ) -> LocalBoxFuture<'_, Option<(Self::RowId, CrdtSequenceMock)>> {
            async move {
                self.0
                    .iter()
                    .enumerate()
                    .find_map(|(idx, value)| (value.0 == id).then_some((idx, *value)))
            }
            .boxed_local()
        }
    }
    impl CrdtWritableSequenceTransaction<CrdtSequenceMock> for CrdtWritableSequenceTransactionMock {
        fn last(&mut self) -> LocalBoxFuture<'_, Option<CrdtWritableSequence>> {
            async move {
                self.0
                    .iter()
                    .max_by_key(|value| value.3)
                    .map(|value| value.crdt())
            }
            .boxed_local()
        }
    }

    mod when_pushing_a_new_value {
        use super::*;

        #[tokio::test]
        async fn on_an_empty_list_the_value_index_is_one() {
            let mut list = CrdtWritableSequenceTransactionMock::default();
            let item = CrdtSequenceMock('a', 0, 0, 0);

            assert_eq!(
                list.push(Author(5), item).await,
                CrdtSequenceMock('a', 1, 5, 1)
            );

            assert_eq!(list.0, vec![CrdtSequenceMock('a', 1, 5, 1)])
        }

        #[tokio::test]
        async fn on_list_with_elements_will_be_one_plus_the_last_element() {
            let mut list =
                CrdtWritableSequenceTransactionMock(vec![CrdtSequenceMock('b', 1, 7, 1)]);

            let item = CrdtSequenceMock('a', 0, 0, 0);

            assert_eq!(
                list.push(Author(5), item).await,
                CrdtSequenceMock('a', 1, 5, 2)
            );

            assert_eq!(
                list.0,
                vec![
                    CrdtSequenceMock('b', 1, 7, 1),
                    CrdtSequenceMock('a', 1, 5, 2)
                ]
            )
        }

        #[tokio::test]
        async fn if_pushing_an_already_existent_it_goes_to_end_of_list() {
            let mut list =
                CrdtWritableSequenceTransactionMock(vec![CrdtSequenceMock('a', 3, 7, 1)]);

            let item = CrdtSequenceMock('a', 0, 0, 0);

            assert_eq!(
                list.push(Author(5), item).await,
                CrdtSequenceMock('a', 4, 5, 2)
            );

            assert_eq!(list.0, vec![CrdtSequenceMock('a', 4, 5, 2),])
        }
    }
}
