//! JSON resource codec sharing the server-function byte decoder.

use codee::Decoder;
use codee::Encoder;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub(crate) struct JsonSliceCodec;

impl<T: Serialize> Encoder<T> for JsonSliceCodec {
	type Encoded = String;
	type Error = serde_json::Error;

	fn encode(value: &T) -> Result<Self::Encoded, Self::Error> {
		serde_json::to_string(value)
	}
}

impl<T: DeserializeOwned> Decoder<T> for JsonSliceCodec {
	type Encoded = str;
	type Error = serde_json::Error;

	fn decode(value: &Self::Encoded) -> Result<T, Self::Error> {
		serde_json::from_slice(value.as_bytes())
	}
}

#[cfg(test)]
#[path = "__tests__/resource_codec_tests.rs"]
mod tests;
