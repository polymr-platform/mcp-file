use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Deserialize)]
pub struct Request {
	pub id: Value,
	pub method: String,
	#[serde(default)]
	pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct Response {
	pub id: Value,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub result: Option<Value>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<ErrorObject>,
}

#[derive(Debug, Serialize)]
pub struct ErrorObject {
	pub code: i64,
	pub message: String,
}

impl Response {
	pub fn ok(id: Value, result: Value) -> Self {
		Self {
			id,
			result: Some(result),
			error: None
		}
	}
	pub fn ok_with_meta(id: Value, result: Value, meta: Value) -> Self {
		let result = insert_meta(result, meta);
		Self {
			id,
			result: Some(result),
			error: None
		}
	}
	pub fn err(id: Value, code: i64, message: impl Into<String>) -> Self {
		Self {
			id,
			result: None,
			error: Some(ErrorObject {
				code,
				message: message.into()
			})
		}
	}
}

fn insert_meta(result: Value, meta: Value) -> Value {
	match result {
		Value::Object(mut map) => {
			map.remove("_meta");
			let mut ordered = Map::new();
			for (key, value) in map {
				ordered.insert(key, value);
			}
			ordered.insert("_meta".to_string(), meta);
			Value::Object(ordered)
		}
		other => {
			let mut map = Map::new();
			map.insert("value".to_string(), other);
			map.insert("_meta".to_string(), meta);
			Value::Object(map)
		}
	}
}
