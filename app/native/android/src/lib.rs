//! Platform host registration generated from this runner's dependencies.
//!
//! `leptoskit native prepare android` writes
//! `../.leptoskit/android/registrant.rs`.

#[cfg(target_os = "android")]
include!(concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/../.leptoskit/android/registrant.rs"
));

#[cfg(not(target_os = "android"))]
const _: &str = "run leptoskit native prepare android to generate registrant.rs";
