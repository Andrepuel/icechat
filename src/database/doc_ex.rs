use super::error::{DataType, DatabaseResult, UnexpectedType};
use automerge::{ObjId, ObjType, Prop, ReadDoc, ScalarValue, Value};
use std::borrow::Cow;

impl<T: ReadDoc> ReadDocEx for T {}
pub trait ReadDocEx: ReadDoc {
    fn get_scalar<O: AsRef<ObjId>, P: Into<Prop>>(
        &self,
        obj: O,
        prop: P,
        expected: DataType,
    ) -> DatabaseResult<Cow<'_, ScalarValue>> {
        self.get_opt_scalar(obj, prop, expected)?
            .ok_or_else(|| UnexpectedType::from_nil(expected).into())
    }

    fn get_opt_scalar<O: AsRef<ObjId>, P: Into<Prop>>(
        &self,
        obj: O,
        prop: P,
        expected: DataType,
    ) -> DatabaseResult<Option<Cow<'_, ScalarValue>>> {
        let value = self.get(obj, prop)?;
        let Some(value) = value else { return Ok(None) };

        match value.0 {
            Value::Scalar(scalar) => Ok(Some(scalar)),
            actual => Err(UnexpectedType::from_actual(actual, expected).into()),
        }
    }

    fn get_map<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> DatabaseResult<ObjId> {
        self.get_opt_map(obj, prop)?
            .ok_or_else(|| UnexpectedType::from_nil(DataType::Map).into())
    }

    fn get_opt_map<O: AsRef<ObjId>, P: Into<Prop>>(
        &self,
        obj: O,
        prop: P,
    ) -> DatabaseResult<Option<ObjId>> {
        let value = self.get(obj, prop)?;
        let Some(value) = value else { return Ok(None);};
        match value {
            (Value::Object(ObjType::Map), id) => Ok(Some(id)),
            (actual, _) => Err(UnexpectedType::from_actual(actual, DataType::Map).into()),
        }
    }

    fn get_list<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> DatabaseResult<ObjId> {
        self.get_opt_list(obj, prop)?
            .ok_or_else(|| UnexpectedType::from_nil(DataType::List).into())
    }

    fn get_opt_list<O: AsRef<ObjId>, P: Into<Prop>>(
        &self,
        obj: O,
        prop: P,
    ) -> DatabaseResult<Option<ObjId>> {
        let obj = self.get(obj, prop)?;
        let Some(obj) = obj else { return Ok(None); };

        match obj {
            (Value::Object(ObjType::List), id) => Ok(Some(id)),
            (actual, _) => Err(UnexpectedType::from_actual(actual, DataType::List).into()),
        }
    }

    fn get_string<O: AsRef<ObjId>, P: Into<Prop>>(
        &self,
        obj: O,
        prop: P,
    ) -> DatabaseResult<String> {
        let expected = DataType::Str;
        self.get_scalar(obj, prop.into(), expected)?
            .into_owned()
            .into_string()
            .map_err(|actual| UnexpectedType::from_actual_scalar(&actual, expected).into())
    }

    fn get_bytes<O: AsRef<ObjId>, P: Into<Prop>>(
        &self,
        obj: O,
        prop: P,
    ) -> DatabaseResult<Vec<u8>> {
        let expected = DataType::Bytes;
        self.get_scalar(obj, prop.into(), expected)?
            .into_owned()
            .into_bytes()
            .map_err(|actual| UnexpectedType::from_actual_scalar(&actual, expected).into())
    }

    fn get_opt_bytes<O: AsRef<ObjId>, P: Into<Prop>>(
        &self,
        obj: O,
        prop: P,
    ) -> DatabaseResult<Option<Vec<u8>>> {
        let expected = DataType::Bytes;
        let Some(scalar) = self.get_opt_scalar(obj, prop, expected)? else { return Ok(None); };

        scalar
            .into_owned()
            .into_bytes()
            .map(Some)
            .map_err(|actual| UnexpectedType::from_actual_scalar(&actual, expected).into())
    }

    fn get_u64<O: AsRef<ObjId>, P: Into<Prop>>(&self, obj: O, prop: P) -> DatabaseResult<u64> {
        let expected = DataType::Uint;
        match self.get_scalar(obj, prop.into(), expected)?.as_ref() {
            ScalarValue::Uint(v) => Ok(*v),
            actual => Err(UnexpectedType::from_actual_scalar(actual, expected).into()),
        }
    }
}
