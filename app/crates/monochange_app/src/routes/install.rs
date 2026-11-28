//! `/install`

use leptoskit_router::__private::Element;

pub fn page() -> Element {
	crate::routes::_view::deferred_to_leptos()
}
