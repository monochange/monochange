//! Platform host registration generated from this runner's dependencies.
//!
//! `leptoskit native prepare ios` / `ios-simulator` writes
//! `../.leptoskit/ios/registrant.rs`.

#[cfg(target_os = "ios")]
include!(concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/../.leptoskit/ios/registrant.rs"
));

#[cfg(not(target_os = "ios"))]
const _: &str = "run leptoskit native prepare ios to generate registrant.rs";
