//! Add release-note output identity to release records created before streams.

use serde_json::Value;

use crate::SchemaError;
use crate::object_mut;

pub(crate) fn apply(value: &mut Value) -> Result<(), SchemaError> {
	let object = object_mut(value)?;
	let Some(changelogs) = object.get_mut("changelogs").and_then(Value::as_array_mut) else {
		return Ok(());
	};

	for changelog in changelogs {
		let Some(changelog) = changelog.as_object_mut() else {
			continue;
		};
		changelog
			.entry("output")
			.or_insert_with(|| Value::String("default".to_string()));
		changelog
			.entry("stream")
			.or_insert_with(|| Value::String("default".to_string()));
	}

	Ok(())
}
