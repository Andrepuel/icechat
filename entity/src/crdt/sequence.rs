use super::{writable::CrdtWritable, Author, CrdtOrd};
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
