use rstest::rstest;

use super::MAX_DESCRIPTION_CHARS;
use super::MAX_NAME_CHARS;
use super::ProjectInputError;
use super::project_slug;
use super::validate_project;

#[rstest]
#[case("Invoices", "invoices")]
#[case("Invoices & Billing!", "invoices-billing")]
#[case("  web -- app  ", "web-app")]
#[case("Café Bar 2", "caf-bar-2")]
#[case("---", "")]
#[case("", "")]
fn slugs_are_lower_case_words_joined_by_hyphens(#[case] name: &str, #[case] slug: &str) {
	assert_eq!(project_slug(name), slug);
}

fn repositories(names: &[&str]) -> Vec<String> {
	names.iter().map(|name| (*name).to_owned()).collect()
}

#[rstest]
fn valid_projects_are_normalized() {
	let input = validate_project(
		"  Invoices  ",
		"  Billing apps ",
		repositories(&["acme/web", " acme/api ", "ACME/web", ""]),
	)
	.unwrap();
	assert_eq!(input.name, "Invoices");
	assert_eq!(input.slug, "invoices");
	assert_eq!(input.description, "Billing apps");
	assert_eq!(input.repositories, ["acme/api", "acme/web"]);
}

#[rstest]
#[case(" ", "", &["acme/api"], ProjectInputError::MissingName)]
#[case("!!!", "", &["acme/api"], ProjectInputError::NameWithoutSlug)]
#[case("Invoices", "", &[], ProjectInputError::NoRepositories)]
#[case("Invoices", "", &[" "], ProjectInputError::NoRepositories)]
fn invalid_projects_explain_what_to_fix(
	#[case] name: &str,
	#[case] description: &str,
	#[case] names: &[&str],
	#[case] error: ProjectInputError,
) {
	assert_eq!(
		validate_project(name, description, repositories(names)),
		Err(error)
	);
}

#[rstest]
fn limits_are_enforced_in_characters() {
	let long_name = "é".repeat(MAX_NAME_CHARS + 1);
	assert_eq!(
		validate_project(&long_name, "", repositories(&["a/b"])),
		Err(ProjectInputError::NameTooLong)
	);
	let long_description = "x".repeat(MAX_DESCRIPTION_CHARS + 1);
	assert_eq!(
		validate_project("Invoices", &long_description, repositories(&["a/b"])),
		Err(ProjectInputError::DescriptionTooLong)
	);
}

#[rstest]
#[case(ProjectInputError::MissingName, "Give the project a name.")]
#[case(
	ProjectInputError::NameTooLong,
	"Keep the project name under 80 characters."
)]
#[case(
	ProjectInputError::NameWithoutSlug,
	"Use letters or numbers in the project name."
)]
#[case(
	ProjectInputError::DescriptionTooLong,
	"Keep the description under 500 characters."
)]
#[case(ProjectInputError::NoRepositories, "Choose at least one repository.")]
fn errors_read_as_instructions(#[case] error: ProjectInputError, #[case] message: &str) {
	assert_eq!(error.to_string(), message);
}
