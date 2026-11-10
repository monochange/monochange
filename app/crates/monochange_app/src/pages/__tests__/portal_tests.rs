use monochange_app_feedback::PublicLink;
use monochange_app_feedback::ShipWindow;

#[test]
fn shipped_and_dated_windows_name_the_release() {
	assert_eq!(
		super::ship_window(&ShipWindow::Released {
			version: "1.2.0".to_owned(),
		})
		.as_deref(),
		Some("Shipped in v1.2.0")
	);
	assert_eq!(
		super::ship_window(&ShipWindow::NextRelease {
			label: Some("Friday".to_owned()),
		})
		.as_deref(),
		Some("Ships Friday")
	);
	assert_eq!(
		super::ship_window(&ShipWindow::NextRelease { label: None }).as_deref(),
		Some("Ships in the next release")
	);

	let (label, url) = super::link_label(&PublicLink::ReleaseNotes(
		"https://example.com/notes".to_owned(),
	));
	assert_eq!(label, "Release notes");
	assert_eq!(url, "https://example.com/notes");
}
