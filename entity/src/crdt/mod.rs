use futures::{future::LocalBoxFuture, FutureExt};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Author(pub i32);

pub trait CrdtValue: Sized {
    type Id;

    fn id(&self) -> Self::Id;
    fn generation(&self) -> i32;
    fn set_generation(&mut self, gen: i32);
    fn author(&self) -> Author;
    fn set_author(&mut self, author: Author);

    fn merge(self, existent: Option<Self>) -> Option<Self> {
        let Some(existent) = existent else { return Some(self) };

        if (self.generation(), self.author()) > (existent.generation(), existent.author()) {
            Some(self)
        } else {
            None
        }
    }
}

pub trait CrdtValueTransaction<V: CrdtValue + 'static> {
    fn add(&mut self, author: Author, mut value: V) -> LocalBoxFuture<'_, V> {
        async move {
            value.set_author(author);
            let gen = self
                .existent(value.id())
                .await
                .map(|existent| existent.generation())
                .unwrap_or(0)
                + 1;
            value.set_generation(gen);

            self.save(value).await
        }
        .boxed_local()
    }

    fn merge(&mut self, value: V) -> LocalBoxFuture<'_, Option<V>> {
        async move {
            let existent = self.existent(value.id()).await;
            let value = value.merge(existent)?;

            Some(self.save(value).await)
        }
        .boxed_local()
    }

    fn save(&mut self, value: V) -> LocalBoxFuture<'_, V>;
    fn existent(&mut self, id: <V as CrdtValue>::Id) -> LocalBoxFuture<'_, Option<V>>;
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    struct CrdtValueMock(usize, i32, i32);
    impl CrdtValue for CrdtValueMock {
        type Id = usize;

        fn id(&self) -> Self::Id {
            0
        }

        fn generation(&self) -> i32 {
            self.1
        }

        fn set_generation(&mut self, gen: i32) {
            self.1 = gen;
        }

        fn author(&self) -> Author {
            Author(self.2)
        }

        fn set_author(&mut self, author: Author) {
            self.2 = author.0;
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
        fn save(&mut self, value: V) -> LocalBoxFuture<'_, V> {
            async move {
                let existent = self.0.iter_mut().find(|x| x.id() == value.id());

                match existent {
                    Some(existent) => *existent = value.clone(),
                    None => self.0.push(value.clone()),
                }

                value
            }
            .boxed_local()
        }

        fn existent(&mut self, id: V::Id) -> LocalBoxFuture<'_, Option<V>> {
            async move { self.0.iter_mut().find(|x| x.id() == id).cloned() }.boxed_local()
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

            assert_eq!(my_value.merge(Some(my_value)), None);
        }

        #[test]
        fn the_bigger_generation_will_wins_the_merge() {
            let bigger_gen = CrdtValueMock(0, 10, 100);
            let smaller_gen = CrdtValueMock(0, 9, 1);

            assert_eq!(bigger_gen.merge(Some(smaller_gen)), Some(bigger_gen));
            assert_eq!(smaller_gen.merge(Some(bigger_gen)), None);
        }

        #[test]
        fn if_the_generation_is_same_the_bigger_author_wins_the_merge() {
            let bigger_author = CrdtValueMock(0, 0, 100);
            let smaller_author = CrdtValueMock(0, 0, 10);

            assert_eq!(
                bigger_author.merge(Some(smaller_author)),
                Some(bigger_author)
            );
            assert_eq!(smaller_author.merge(Some(bigger_author)), None);
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
