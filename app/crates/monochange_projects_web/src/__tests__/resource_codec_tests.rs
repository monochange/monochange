use codee::Decoder;
use codee::Encoder;
use codee::string::JsonSerdeCodec;

use super::JsonSliceCodec;

#[test]
fn codec_round_trips_json_and_rejects_malformed_input() {
	let value = vec!["hello", "世界"];
	let encoded = JsonSliceCodec::encode(&value).unwrap();
	let decoded: Vec<String> = JsonSliceCodec::decode(&encoded).unwrap();
	assert_eq!(decoded, value);
	assert!(<JsonSliceCodec as Decoder<Vec<String>>>::decode("[").is_err());
}

#[test]
fn codec_interoperates_with_existing_resource_json() {
	for value in [
		Ok(Some(vec!["世界".to_owned()])),
		Ok(None),
		Err("failed".to_owned()),
	] {
		let original = JsonSerdeCodec::encode(&value).unwrap();
		let decoded: Result<Option<Vec<String>>, String> =
			JsonSliceCodec::decode(&original).unwrap();
		assert_eq!(decoded, value);

		let replacement = JsonSliceCodec::encode(&value).unwrap();
		let decoded: Result<Option<Vec<String>>, String> =
			JsonSerdeCodec::decode(&replacement).unwrap();
		assert_eq!(decoded, value);
	}
}
