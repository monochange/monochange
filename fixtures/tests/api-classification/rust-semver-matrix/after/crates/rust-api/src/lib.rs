pub trait StableApi {
	fn stable(&self) -> bool;
}

#[cfg(feature = "experimental")]
pub trait ExperimentalApi {
	fn retained(&self);
}
