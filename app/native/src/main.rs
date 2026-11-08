//! Scaffold server binary for native RPC development.
//!
//! Full axum + SQLite mounting needs the leptoskit db packs and is intended to
//! run via `leptoskit dev` once the CLI is installed. This binary keeps a
//! compile-checked entry that registers RPC ops and prints next steps.

fn main() {
	monochange_native_backend::register_rpc_ops();
	println!(
		"monochange native scaffold\n\
		 - RPC: auth_handshake (SessionProbe), list_projects\n\
		 - Run with: cd app/native && leptoskit doctor && leptoskit native prepare android\n\
		 - Android needs ANDROID_HOME (SDK 36 + NDK); iOS needs macOS/Xcode\n\
		 - See app/native/README.md"
	);
}
