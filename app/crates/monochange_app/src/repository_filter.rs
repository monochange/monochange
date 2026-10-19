//! Narrowing a long repository list: search by name, keep one account, and
//! page through what is left.
//!
//! The filters live in the page's query string, so a filtered view survives
//! a reload and can be shared. Pages stay local to each list.

use leptos_router::location::Url;

use crate::server_fns::repos::RepoInfo;

/// How many repositories one page of a list shows.
pub const REPOSITORIES_PER_PAGE: usize = 20;

/// The query-string key for the name search.
pub const SEARCH_KEY: &str = "q";

/// The query-string key for the account filter.
pub const ORGANIZATION_KEY: &str = "org";

/// The dashboard's repository filters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryFilter {
	/// The search exactly as typed; matching ignores case and outer spaces.
	pub search: String,
	/// Only this account's repositories, or every account when `None`.
	pub organization: Option<String>,
}

impl RepositoryFilter {
	/// Filters from query-string values, treating blank values as absent.
	pub fn new(search: Option<String>, organization: Option<String>) -> Self {
		Self {
			search: search.unwrap_or_default(),
			organization: organization.filter(|login| !login.trim().is_empty()),
		}
	}

	/// Whether anything narrows the list.
	pub fn is_active(&self) -> bool {
		!self.search.trim().is_empty() || self.organization.is_some()
	}

	/// Whether the account called `login` passes the account filter.
	pub fn includes_organization(&self, login: &str) -> bool {
		self.organization
			.as_deref()
			.is_none_or(|organization| organization.eq_ignore_ascii_case(login))
	}

	/// The same filters with a different search.
	#[must_use]
	pub fn with_search(&self, search: String) -> Self {
		Self {
			search,
			organization: self.organization.clone(),
		}
	}

	/// The same filters for a different account; blank means every account.
	#[must_use]
	pub fn with_organization(&self, organization: String) -> Self {
		Self::new(Some(self.search.clone()), Some(organization))
	}

	/// `path` with these filters as its query string.
	pub fn href(&self, path: &str) -> String {
		let mut pairs = Vec::new();

		if !self.search.trim().is_empty() {
			pairs.push((SEARCH_KEY, self.search.as_str()));
		}

		if let Some(organization) = &self.organization {
			pairs.push((ORGANIZATION_KEY, organization.as_str()));
		}

		if pairs.is_empty() {
			return path.to_owned();
		}

		// The router's escape uses the browser's encoder in the client bundle.
		let query: Vec<String> = pairs
			.into_iter()
			.map(|(key, value)| format!("{key}={}", Url::escape(value)))
			.collect();
		format!("{path}?{}", query.join("&"))
	}
}

/// Whether `repository` matches `search`.
///
/// A search without a `/` looks at the repository's own name, since the list
/// is already grouped by account. With a `/` it matches `owner/name`, so
/// `acme/` finds every repository of `acme`. GitHub names are ASCII, so
/// ASCII case folding keeps Unicode tables out of the browser bundle.
pub fn matches_search(repository: &RepoInfo, search: &str) -> bool {
	let search = search.trim().to_ascii_lowercase();

	if search.is_empty() {
		return true;
	}

	let full_name = repository.github_full_name.to_ascii_lowercase();
	let haystack = if search.contains('/') {
		full_name.as_str()
	} else {
		full_name
			.rsplit_once('/')
			.map_or(full_name.as_str(), |(_, name)| name)
	};

	haystack.contains(&search)
}

/// One page of the repositories that match a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryPage {
	pub repositories: Vec<RepoInfo>,
	/// The page shown, from 1; a page past the end shows the last one.
	pub page: usize,
	/// How many pages the matches fill, at least 1.
	pub page_count: usize,
	/// How many repositories match the search.
	pub matches: usize,
	/// The 1-based position of the first repository shown, or 0 for none.
	pub first: usize,
	/// The 1-based position of the last repository shown, or 0 for none.
	pub last: usize,
}

impl RepositoryPage {
	pub fn has_previous(&self) -> bool {
		self.page > 1
	}

	pub fn has_next(&self) -> bool {
		self.page < self.page_count
	}
}

/// Page `page` of the repositories that match `search`.
pub fn repository_page(repositories: &[RepoInfo], search: &str, page: usize) -> RepositoryPage {
	let matching: Vec<&RepoInfo> = repositories
		.iter()
		.filter(|repository| matches_search(repository, search))
		.collect();
	let matches = matching.len();
	let page_count = matches.div_ceil(REPOSITORIES_PER_PAGE).max(1);
	let page = page.clamp(1, page_count);
	let start = (page - 1) * REPOSITORIES_PER_PAGE;
	let repositories: Vec<RepoInfo> = matching
		.into_iter()
		.skip(start)
		.take(REPOSITORIES_PER_PAGE)
		.cloned()
		.collect();
	let (first, last) = if repositories.is_empty() {
		(0, 0)
	} else {
		(start + 1, start + repositories.len())
	};

	RepositoryPage {
		repositories,
		page,
		page_count,
		matches,
		first,
		last,
	}
}

/// How many repositories match `search`.
pub fn count_matches(repositories: &[RepoInfo], search: &str) -> usize {
	repositories
		.iter()
		.filter(|repository| matches_search(repository, search))
		.count()
}

/// `count` with the right form of "repository".
pub fn repository_count(count: usize) -> String {
	match count {
		1 => "1 repository".to_owned(),
		count => format!("{count} repositories"),
	}
}

#[cfg(test)]
#[path = "__tests__/repository_filter_tests.rs"]
mod tests;
