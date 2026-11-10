use monochange_extension_shared::OrganizationSummary;
use monochange_extension_shared::ProjectSummary;
use monochange_extension_shared::SessionUser;
use monochange_extension_shared::WebsiteRelease;

use crate::chrome::BUTTON;
use crate::chrome::MC_ACCENT_LIGHT;
use crate::chrome::MC_PAGE_LIGHT;
use crate::chrome::PANEL;
use crate::chrome::REQUIRED_MC_TOKENS;
use crate::chrome::SHELL;
use crate::chrome::THEME_COLORS;
use crate::chrome::leptoskit_toml_declares_theme;
use crate::chrome::theme_maps;
use crate::popup::PopupReady;
use crate::popup::PopupState;
use crate::popup::PopupWireState;
use crate::popup::ProjectRow;

#[test]
fn theme_maps_cover_every_release_floor_token() {
	let (colors, lengths) = theme_maps();
	for token in REQUIRED_MC_TOKENS {
		let name = token.trim_start_matches('-');
		assert!(
			colors.contains_key(name) || lengths.contains_key(name),
			"theme maps missing {token}"
		);
	}
	assert_eq!(
		colors.get("mc-page").map(String::as_str),
		Some(MC_PAGE_LIGHT)
	);
	assert_eq!(
		colors.get("mc-accent").map(String::as_str),
		Some(MC_ACCENT_LIGHT)
	);
	assert_eq!(THEME_COLORS.len(), colors.len());
}

#[test]
fn leptoskit_toml_declares_shared_mc_theme() {
	let toml = include_str!("../../../leptoskit.toml");
	assert!(
		leptoskit_toml_declares_theme(toml),
		"leptoskit.toml [theme] must list Release floor --mc-* tokens"
	);
}

#[test]
fn chrome_uses_tailwind_var_utilities_not_semantic_sheets() {
	for class in [SHELL, PANEL, BUTTON] {
		assert!(
			class.contains("var(--mc-"),
			"expected Tailwind var(--mc-*) utility in {class}"
		);
		assert!(
			!class
				.split_whitespace()
				.any(|part| part.starts_with("mc-") && !part.contains('[') && !part.contains(':')),
			"unexpected semantic mc-* sheet class in {class}"
		);
	}
	let css = include_str!("../../../assets/app.css");
	assert!(
		!css.contains(".mc-shell"),
		"assets/app.css must not define semantic chrome sheets"
	);
}

#[test]
fn popup_ready_flattens_projects_and_status() {
	let ready = PopupReady {
		user: SessionUser {
			github_id: 1,
			github_login: "dev".into(),
			github_avatar_url: None,
			plan_tier: "free".into(),
		},
		organizations: vec![OrganizationSummary {
			login: "dev".into(),
			personal: true,
			avatar_url: None,
			supports_projects: true,
			repositories: Vec::new(),
			projects: vec![ProjectSummary {
				slug: "monochange".into(),
				name: "monochange".into(),
				description: String::new(),
				repository_count: 2,
			}],
		}],
		releases: vec![WebsiteRelease {
			version: "0.3.3".into(),
			summary: Vec::new(),
			sections: Vec::new(),
		}],
	};
	assert_eq!(ready.projects(), 1);
	assert_eq!(ready.status_line(), "v0.3.3 on monochange.dev");
	assert_eq!(
		ready.project_rows(),
		vec![ProjectRow {
			organization: "dev".into(),
			slug: "monochange".into(),
			name: "monochange".into(),
			repository_count: 2,
		}]
	);
	assert_eq!(ready.project_rows()[0].meta(), "dev/monochange · 2 repos");

	let wire = PopupWireState::from(PopupState::Ready(ready.clone()));
	assert_eq!(PopupState::from(wire), PopupState::Ready(ready));
}

#[test]
fn extension_chrome_uses_styles_supported_by_the_runtime() {
	use crate::chrome;
	let mut errors = Vec::new();
	for classes in [
		chrome::SHELL,
		chrome::CHROME,
		chrome::BRAND,
		chrome::EYEBROW,
		chrome::TITLE,
		chrome::COPY,
		chrome::PANEL,
		chrome::ROW,
		chrome::BUTTON_QUIET,
		chrome::ACTIONS,
		chrome::STATUS,
		chrome::STATUS_LIVE,
	] {
		if let Err(error) = leptoskit::prelude::Element::view().try_class(classes) {
			errors.push(error);
		}
	}
	if let Err(error) = leptoskit::prelude::Element::button("Sign in").try_class(chrome::BUTTON) {
		errors.push(error);
	}
	assert!(errors.is_empty(), "{errors:#?}");
}
