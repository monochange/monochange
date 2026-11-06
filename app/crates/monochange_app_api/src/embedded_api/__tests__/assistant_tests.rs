use super::*;

#[test]
fn configuration_requires_an_explicit_model_key_and_secure_endpoint() {
	for (key, model, endpoint) in [
		(None, Some("model"), "https://model.example"),
		(Some(""), Some("model"), "https://model.example"),
		(Some("key"), None, "https://model.example"),
		(Some("key"), Some(" "), "https://model.example"),
		(Some("key"), Some("model"), "invalid"),
		(Some("key"), Some("model"), "http://model.example"),
		(Some("key"), Some("model"), "ftp://localhost/completions"),
		(
			Some("key"),
			Some("model"),
			"https://user:pass@model.example",
		),
	] {
		assert!(AssistantConfig::configured(key, model, endpoint).is_none());
	}
	for endpoint in [
		"https://model.example/completions",
		"http://localhost:8080/completions",
	] {
		let config = AssistantConfig::configured(Some("key"), Some("model"), endpoint).unwrap();
		assert_eq!(config.endpoint, endpoint);
		assert_eq!(config.model, "model");
	}
}
