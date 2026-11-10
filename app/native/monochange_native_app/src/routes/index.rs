//! `/` — the project floor.

use leptoskit::prelude::*;

use crate::screens::home::Home;

pub fn page() -> Element {
	view! { <Home /> }
}
