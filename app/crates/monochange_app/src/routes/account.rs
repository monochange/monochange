//! `/account`

use leptos::prelude::Get;
use leptos::prelude::Resource;
use leptoskit::prelude::*;

use crate::ui::heading;
use crate::ui::link;
use crate::ui::text;

pub fn page() -> Element {
	let sessions = crate::slots::once(|| {
		Resource::new_blocking(
			|| (),
			|_| crate::server_fns::session::list_client_sessions(),
		)
	});
	match sessions.get() {
		None => text("Loading devices…"),
		Some(Ok(sessions)) if sessions.is_empty() => {
			let title = heading(1, "Account and devices");
			let empty = text("No connected devices.");
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{title}
					{empty}
				</View>
			}
		}
		Some(Ok(sessions)) => {
			let mut children = vec![heading(1, "Account and devices")];
			for session in sessions {
				children.push(text(format!(
					"{} · expires {}",
					session.client, session.expires_at
				)));
				children.push(text("Revoke"));
			}
			children.push(text("Sign out everywhere"));
			leptoskit::view! { <View class="flex flex-col gap-4">{children}</View> }
		}
		Some(Err(_)) => {
			let title = heading(1, "Account and devices");
			let error = text("Your devices couldn't be loaded. Sign in or reload to try again.");
			let sign_in = link("/login?next=%2Faccount", "Sign in");
			leptoskit::view! {
				<View class="flex flex-col gap-4">
					{title}
					{error}
					{sign_in}
				</View>
			}
		}
	}
}
