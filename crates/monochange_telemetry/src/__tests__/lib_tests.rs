use std::sync::LazyLock;
use std::sync::Mutex;

static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

use std::path::Path;

use super::*;

#[test]
fn telemetry_mode_environment_uses_default_state_file() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(std::sync::PoisonError::into_inner);
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
	let state_home = temp.path().join("state");
	let state_home_value = state_home.to_string_lossy().to_string();
	temp_env::with_vars(
		[
			(TELEMETRY_ENV, Some("local")),
			(TELEMETRY_FILE_ENV, None::<&str>),
			("XDG_STATE_HOME", Some(state_home_value.as_str())),
			("HOME", None::<&str>),
		],
		|| {
			let sink = TelemetrySink::from_env();
			assert!(matches!(sink, TelemetrySink::LocalJsonl { .. }));
			sink.capture_step(StepTelemetry {
				command_name: "step validate",
				step_index: 3,
				step_kind: "Validate",
				skipped: true,
				duration: Duration::from_millis(7),
				outcome: TelemetryOutcome::Skipped,
				error: Some(&MonochangeError::Config(
					"secret path /tmp/repo".to_string(),
				)),
			});
		},
	);

	let events = read_events(&state_home.join("monochange").join("telemetry.jsonl"));
	assert_eq!(events.len(), 1);
	let event = event_at(&events, 0);
	assert_eq!(json_str(event, "/body/string_value"), "command_step");
	assert_eq!(json_str(event, "/attributes/command_name"), "step validate");
	assert_eq!(json_u64(event, "/attributes/step_index"), 3);
	assert_eq!(json_str(event, "/attributes/step_kind"), "Validate");
	assert!(json_bool(event, "/attributes/skipped"));

	assert_eq!(json_u64(event, "/attributes/duration_ms"), 7);
	assert_eq!(json_str(event, "/attributes/outcome"), "skipped");
	assert_eq!(json_str(event, "/attributes/error_kind"), "config_error");
	assert!(!event.to_string().contains("/tmp/repo"));
}

fn event_at(events: &[serde_json::Value], index: usize) -> &serde_json::Value {
	events
		.get(index)
		.unwrap_or_else(|| panic!("missing telemetry event {index}"))
}

fn json_value<'event>(
	event: &'event serde_json::Value,
	pointer: &str,
) -> &'event serde_json::Value {
	event
		.pointer(pointer)
		.unwrap_or_else(|| panic!("missing json pointer {pointer} in {event}"))
}

fn json_str<'event>(event: &'event serde_json::Value, pointer: &str) -> &'event str {
	json_value(event, pointer)
		.as_str()
		.unwrap_or_else(|| panic!("json pointer {pointer} is not a string in {event}"))
}

fn json_bool(event: &serde_json::Value, pointer: &str) -> bool {
	json_value(event, pointer)
		.as_bool()
		.unwrap_or_else(|| panic!("json pointer {pointer} is not a bool in {event}"))
}

fn json_u64(event: &serde_json::Value, pointer: &str) -> u64 {
	json_value(event, pointer)
		.as_u64()
		.unwrap_or_else(|| panic!("json pointer {pointer} is not an unsigned integer in {event}"))
}

fn read_events(path: &Path) -> Vec<serde_json::Value> {
	fs::read_to_string(path)
		.unwrap_or_else(|error| panic!("telemetry file should be written: {error}"))
		.lines()
		.map(|line| {
			serde_json::from_str(line).unwrap_or_else(|error| panic!("valid json event: {error}"))
		})
		.collect()
}
