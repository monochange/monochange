use std::sync::LazyLock;
use std::sync::Mutex;

pub(crate) use super::cli_runtime::lookup_template_value;
pub(crate) use super::cli_runtime::parse_direct_template_reference;
pub(crate) use super::cli_runtime::template_value_to_input_values;
use super::*;

pub(crate) static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub(crate) fn block_on_in_context<T>(future: impl std::future::Future<Output = T>) -> T {
	match tokio::runtime::Handle::try_current() {
		Ok(handle) => tokio::task::block_in_place(|| handle.block_on(future)),
		Err(_) => tokio::runtime::Runtime::new().unwrap().block_on(future),
	}
}

#[path = "lib_tests.rs"]
mod lib_tests;

#[path = "sync_tests.rs"]
mod sync_tests;
