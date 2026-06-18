use std::path::PathBuf;

fn target_profile_dir() -> PathBuf {
	let mut current_exe = std::env::current_exe()
		.unwrap_or_else(|error| panic!("resolve current test executable path: {error}"));

	while let Some(file_name) = current_exe.file_name().and_then(|value| value.to_str()) {
		if file_name == "debug" || file_name == "release" {
			break;
		}
		current_exe.pop();
	}

	current_exe
}

#[test]
fn get_cargo_bin_builds_workspace_binary_when_missing() {
	let binary_name = if cfg!(windows) { "xtask.exe" } else { "xtask" };
	let binary_path = target_profile_dir().join(binary_name);
	let _ = std::fs::remove_file(&binary_path);

	let resolved = super::get_cargo_bin("xtask");

	assert_eq!(resolved, binary_path);
	assert!(
		resolved.exists(),
		"expected `{}` to exist",
		resolved.display()
	);
}
