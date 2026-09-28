use serde::{Serialize, Serializer};
use serde::ser::{SerializeSeq, SerializeTuple, SerializeTupleStruct, SerializeMap, SerializeStruct};
use serde_with::{serde_as, KeyValueMap};
#[derive(Clone, Copy)]
enum Shape { Seq, UnknownSeq, Tuple, TupleStruct, Map, UnknownMap, Struct }
struct Empty(Shape);
impl Serialize for Empty {
 fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok,S::Error> {
  match self.0 {
   Shape::Seq => s.serialize_seq(Some(0))?.end(),
   Shape::UnknownSeq => s.serialize_seq(None)?.end(),
   Shape::Tuple => s.serialize_tuple(0)?.end(),
   Shape::TupleStruct => s.serialize_tuple_struct("Empty",0)?.end(),
   Shape::Map => s.serialize_map(Some(0))?.end(),
   Shape::UnknownMap => s.serialize_map(None)?.end(),
   Shape::Struct => s.serialize_struct("Empty",0)?.end(),
  }
 }
}
#[serde_as]
#[derive(Serialize)]
#[serde(transparent)]
struct Map<T: Serialize>(#[serde_as(as="KeyValueMap<_>")] Vec<T>);
fn rejects(shape: Shape) { assert!(serde_json::to_string(&Map(vec![Empty(shape)])).is_err()); }
#[test] fn empty_sequence_returns_error() { rejects(Shape::Seq); }
#[test] fn unknown_sequence_returns_error() { rejects(Shape::UnknownSeq); }
#[test] fn empty_tuple_returns_error() { rejects(Shape::Tuple); }
#[test] fn empty_tuple_struct_returns_error() { rejects(Shape::TupleStruct); }
#[test] fn empty_map_returns_error() { rejects(Shape::Map); }
#[test] fn unknown_map_returns_error() { rejects(Shape::UnknownMap); }
#[test] fn empty_struct_returns_error() { rejects(Shape::Struct); }
#[test] fn valid_key_value_sequence_preserved() {
 assert_eq!(serde_json::to_string(&Map(vec![("a", 42)])).unwrap(), r#"{"a":[42]}"#);
}
#[test] fn valid_struct_preserved() {
 #[derive(Serialize)] struct Entry { #[serde(rename="$key$")] key: &'static str, value: u32 }
 assert_eq!(serde_json::to_string(&Map(vec![Entry{key:"a",value:42}])).unwrap(), r#"{"a":{"value":42}}"#);
}
