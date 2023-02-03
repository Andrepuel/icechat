use automerge::{ObjId, ObjType, Prop, ReadDoc, ScalarValue, Value};
use std::borrow::Cow;

impl<T: ReadDoc> ReadDocEx for T {}
pub trait ReadDocEx: ReadDoc {
    fn get_scalar<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> Cow<'_, ScalarValue> {
        match self.get(obj, prop).unwrap().unwrap().0 {
            Value::Scalar(scalar) => scalar,
            _ => panic!(),
        }
    }

    fn get_map<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> ObjId {
        self.get_opt_map(obj, prop).unwrap()
    }

    fn get_opt_map<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> Option<ObjId> {
        match self.get(obj, prop).unwrap()? {
            (Value::Object(ObjType::Map), id) => Some(id),
            _ => panic!(),
        }
    }

    fn get_list<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> (ObjId, usize) {
        let obj = match self.get(obj, prop).unwrap().unwrap() {
            (Value::Object(ObjType::List), id) => id,
            _ => panic!(),
        };

        let length = self.length(&obj);
        (obj, length)
    }

    fn get_string<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> String {
        self.get_scalar(obj, prop.into())
            .into_owned()
            .into_string()
            .unwrap()
    }

    fn get_bytes<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> Vec<u8> {
        self.get_scalar(obj, prop.into())
            .into_owned()
            .into_bytes()
            .unwrap()
    }

    fn get_u64<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> u64 {
        match self.get_scalar(obj, prop.into()).as_ref() {
            ScalarValue::Uint(v) => *v,
            _ => panic!(),
        }
    }
}
