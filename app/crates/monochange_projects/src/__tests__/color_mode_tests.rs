use leptos::prelude::*;

use super::ColorMode;
use super::provide_color_mode;
use super::use_color_mode;

#[test]
fn server_render_starts_light_and_controls_share_the_same_preference() {
	Owner::new().with(|| {
		let state = provide_color_mode();
		assert_eq!(state.mode.get(), ColorMode::Light);

		state.toggle.run(());
		assert_eq!(use_color_mode().mode.get(), ColorMode::Dark);

		state.toggle.run(());
		assert_eq!(state.mode.get(), ColorMode::Light);

		state.set_mode.run(ColorMode::Dark);
		assert_eq!(use_color_mode().mode.get(), ColorMode::Dark);
	});
}
