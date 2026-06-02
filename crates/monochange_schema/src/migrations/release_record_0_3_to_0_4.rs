//! Migrate release records from camelCase object keys to `snake_case` object keys.

use serde_json::Value;

use crate::SchemaError;
use crate::object_mut;

pub(crate) fn apply(value: &mut Value) -> Result<(), SchemaError> {
	object_mut(value)?;
	normalize_object_keys(value);
	Ok(())
}

fn normalize_object_keys(value: &mut Value) {
	match value {
		Value::Object(object) => {
			let entries = std::mem::take(object);
			for (key, mut value) in entries {
				normalize_object_keys(&mut value);
				object.insert(camel_to_snake(&key), value);
			}
		}
		Value::Array(values) => {
			for value in values {
				normalize_object_keys(value);
			}
		}
		Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
	}
}

fn camel_to_snake(value: &str) -> String {
	let mut output = String::with_capacity(value.len());
	for character in value.chars() {
		if character.is_ascii_uppercase() {
			if !output.is_empty() {
				output.push('_');
			}
			output.push(character.to_ascii_lowercase());
		} else {
			output.push(character);
		}
	}
	output
}
