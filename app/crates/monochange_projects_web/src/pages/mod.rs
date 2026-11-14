//! The moved lazy-route pages of the projects browser entry.
//!
//! Each page keeps its `#[lazy_route]` split in this entry crate: the
//! generated wasm-split exports must not exist in any client that does not
//! load this bundle (see the crate documentation).

pub mod feedback_console;
pub mod home;
pub mod organization;
pub mod portal;
pub mod project;
