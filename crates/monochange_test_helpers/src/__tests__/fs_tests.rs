use super::*;

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
	match payload.downcast::<String>() {
		Ok(message) => *message,
		Err(payload) => {
			match payload.downcast::<&'static str>() {
				Ok(message) => (*message).to_string(),
				Err(_) => "non-string panic payload".to_string(),
			}
		}
	}
}

#[test]
fn copy_directory_reports_copy_failures() {
	let source = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let source_file = source.path().join("file.txt");
	let destination_conflict = destination.path().join("file.txt");

	fs::write(&source_file, "hello")
		.unwrap_or_else(|error| panic!("write source file {}: {error}", source_file.display()));
	fs::create_dir_all(&destination_conflict).unwrap_or_else(|error| {
		panic!(
			"create destination conflict {}: {error}",
			destination_conflict.display()
		)
	});

	let panic = std::panic::catch_unwind(|| copy_directory(source.path(), destination.path()))
		.err()
		.unwrap_or_else(|| panic!("expected copy failure panic"));
	let message = panic_message(panic);

	assert!(message.contains("copy"), "panic message: {message}");
	assert!(message.contains("file.txt"), "panic message: {message}");
}

#[test]
fn copy_directory_refreshes_modification_times() {
	// Fixtures overwrite a `before` copy with an `after` copy whose files have
	// the same size. `fs::copy` preserves the source mtime, so git's stat cache
	// could treat the second copy as unchanged and commit stale contents.
	let source = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let source_file = source.path().join("manifest.txt");
	fs::write(&source_file, "version = \"0.1.0\"\n")
		.unwrap_or_else(|error| panic!("write source file: {error}"));

	// Backdate the source so a preserved mtime is unambiguously older.
	let file = fs::File::options()
		.write(true)
		.open(&source_file)
		.unwrap_or_else(|error| panic!("open source file: {error}"));
	file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600))
		.unwrap_or_else(|error| panic!("set source mtime: {error}"));
	drop(file);

	copy_directory(source.path(), destination.path());

	let copied = destination.path().join("manifest.txt");
	let copied_mtime = fs::metadata(&copied)
		.unwrap_or_else(|error| panic!("metadata copied file: {error}"))
		.modified()
		.unwrap_or_else(|error| panic!("read copied mtime: {error}"));
	let source_mtime = fs::metadata(&source_file)
		.unwrap_or_else(|error| panic!("metadata source file: {error}"))
		.modified()
		.unwrap_or_else(|error| panic!("read source mtime: {error}"));

	assert!(
		copied_mtime > source_mtime,
		"copied file must get a fresh mtime, copied={copied_mtime:?} source={source_mtime:?}"
	);
}

#[test]
fn copy_directory_survives_files_that_cannot_be_reopened_for_writing() {
	// The mtime refresh must never fail the copy. Copying a file that cannot be
	// reopened for writing still produces the destination file.
	let source = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let locked = source.path().join("locked.txt");
	fs::write(&locked, "contents").unwrap_or_else(|error| panic!("write locked file: {error}"));

	let mut permissions = fs::metadata(&locked)
		.unwrap_or_else(|error| panic!("metadata locked file: {error}"))
		.permissions();
	permissions.set_readonly(true);
	fs::set_permissions(&locked, permissions)
		.unwrap_or_else(|error| panic!("make locked file readonly: {error}"));

	copy_directory(source.path(), destination.path());

	assert_eq!(
		fs::read_to_string(destination.path().join("locked.txt"))
			.unwrap_or_else(|error| panic!("read copied file: {error}")),
		"contents"
	);
}
