pub fn content_text(result: &rmcp::model::CallToolResult) -> String {
	let Some(content) = result.content.first() else {
		return String::new();
	};

	match content {
		rmcp::model::ContentBlock::Text(text) => text.text.clone(),
		_ => String::new(),
	}
}
