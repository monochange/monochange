//! Project input rules. The limits are shared with the browser, which
//! applies them as form attributes; the server validates every submission.

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

pub const MAX_NAME_CHARS: usize = 80;
pub const MAX_DESCRIPTION_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Error, Serialize, Deserialize)]
pub enum ProjectInputError {
	#[error("Give the project a name.")]
	MissingName,
	#[error("Keep the project name under {MAX_NAME_CHARS} characters.")]
	NameTooLong,
	#[error("Use letters or numbers in the project name.")]
	NameWithoutSlug,
	#[error("Keep the description under {MAX_DESCRIPTION_CHARS} characters.")]
	DescriptionTooLong,
	#[error("Choose at least one repository.")]
	NoRepositories,
}

/// A validated project, ready to store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInput {
	pub name: String,
	pub slug: String,
	pub description: String,
	/// Repository full names, deduplicated and sorted.
	pub repositories: Vec<String>,
}

/// The URL segment for a project name: lower-case ASCII letters and digits
/// separated by single hyphens. `"Invoices & Billing!"` becomes
/// `"invoices-billing"`.
pub fn project_slug(name: &str) -> String {
	let mut slug = String::with_capacity(name.len());
	for character in name.chars() {
		if character.is_ascii_alphanumeric() {
			slug.push(character.to_ascii_lowercase());
		} else if !slug.is_empty() && !slug.ends_with('-') {
			slug.push('-');
		}
	}
	slug.trim_end_matches('-').to_owned()
}

/// Checks a project form and normalizes its values.
pub fn validate_project(
	name: &str,
	description: &str,
	repositories: Vec<String>,
) -> Result<ProjectInput, ProjectInputError> {
	let name = name.trim();
	if name.is_empty() {
		return Err(ProjectInputError::MissingName);
	}
	if name.chars().count() > MAX_NAME_CHARS {
		return Err(ProjectInputError::NameTooLong);
	}
	let slug = project_slug(name);
	if slug.is_empty() {
		return Err(ProjectInputError::NameWithoutSlug);
	}
	let description = description.trim();
	if description.chars().count() > MAX_DESCRIPTION_CHARS {
		return Err(ProjectInputError::DescriptionTooLong);
	}
	let mut repositories: Vec<String> = repositories
		.into_iter()
		.map(|repository| repository.trim().to_owned())
		.filter(|repository| !repository.is_empty())
		.collect();
	repositories.sort_by_key(|repository| repository.to_ascii_lowercase());
	repositories.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
	if repositories.is_empty() {
		return Err(ProjectInputError::NoRepositories);
	}
	Ok(ProjectInput {
		name: name.to_owned(),
		slug,
		description: description.to_owned(),
		repositories,
	})
}

#[cfg(test)]
#[path = "__tests__/projects_tests.rs"]
mod tests;
