//! Repository filters narrow by name and account, and page what is left.

use rstest::rstest;

use super::*;

fn repository(full_name: &str) -> RepoInfo {
	RepoInfo {
		id: 1,
		github_full_name: full_name.to_owned(),
		github_private: false,
		plan_tier: "free".to_owned(),
		installation_login: full_name.split('/').next().unwrap().to_owned(),
		installation_suspended: false,
	}
}

fn numbered(count: usize) -> Vec<RepoInfo> {
	(1..=count)
		.map(|number| repository(&format!("acme/repo-{number:03}")))
		.collect()
}

#[test]
fn blank_values_mean_no_filter() {
	let filter = RepositoryFilter::new(None, Some("  ".to_owned()));

	assert_eq!(filter, RepositoryFilter::default());
	assert!(!filter.is_active());
	assert!(!RepositoryFilter::new(Some("  ".to_owned()), None).is_active());
	assert!(filter.includes_organization("anyone"));
	assert_eq!(filter.href("/dashboard"), "/dashboard");
}

#[test]
fn filters_round_trip_through_the_query_string() {
	let filter = RepositoryFilter::new(Some("web app".to_owned()), Some("Acme".to_owned()));

	assert!(filter.is_active());
	assert!(filter.includes_organization("acme"));
	assert!(!filter.includes_organization("globex"));
	assert_eq!(filter.href("/dashboard"), "/dashboard?q=web+app&org=Acme");
	assert_eq!(
		filter.with_search(String::new()).href("/dashboard"),
		"/dashboard?org=Acme"
	);
	assert_eq!(
		filter.with_organization(String::new()).href("/dashboard"),
		"/dashboard?q=web+app"
	);
	assert_eq!(
		RepositoryFilter::new(None, Some("Acme".to_owned())).with_organization("globex".to_owned()),
		RepositoryFilter::new(None, Some("globex".to_owned()))
	);
}

#[rstest]
#[case::empty("", true)]
#[case::name("WEB", true)]
#[case::outer_spaces("  web ", true)]
#[case::owner_is_not_a_name("acme", false)]
#[case::owner_with_slash("acme/", true)]
#[case::full_name("acme/web", true)]
#[case::other_owner("globex/", false)]
#[case::missing("api", false)]
fn search_matches_names(#[case] search: &str, #[case] expected: bool) {
	assert_eq!(
		matches_search(&repository("acme/web-app"), search),
		expected
	);
}

#[test]
fn a_name_without_an_owner_is_searched_whole() {
	assert!(matches_search(&repository("standalone"), "stand"));
}

#[test]
fn pages_hold_a_fixed_number_of_repositories() {
	let repositories = numbered(45);
	let first = repository_page(&repositories, "", 1);

	assert_eq!(first.page_count, 3);
	assert_eq!(first.matches, 45);
	assert_eq!((first.first, first.last), (1, REPOSITORIES_PER_PAGE));
	assert_eq!(first.repositories.len(), REPOSITORIES_PER_PAGE);
	assert!(!first.has_previous());
	assert!(first.has_next());

	let last = repository_page(&repositories, "", 3);

	assert_eq!((last.first, last.last), (41, 45));
	assert_eq!(last.repositories[0].github_full_name, "acme/repo-041");
	assert!(last.has_previous());
	assert!(!last.has_next());
}

#[test]
fn out_of_range_pages_are_clamped() {
	let repositories = numbered(25);

	assert_eq!(repository_page(&repositories, "", 0).page, 1);
	assert_eq!(repository_page(&repositories, "", 9).page, 2);
}

#[test]
fn searches_page_only_their_matches() {
	let repositories = numbered(45);
	let page = repository_page(&repositories, "repo-04", 1);

	assert_eq!(page.matches, 6);
	assert_eq!(page.page_count, 1);
	assert_eq!(count_matches(&repositories, "repo-04"), 6);
}

#[test]
fn an_empty_search_result_has_one_empty_page() {
	let page = repository_page(&numbered(3), "missing", 4);

	assert_eq!(page.page, 1);
	assert_eq!(page.page_count, 1);
	assert_eq!((page.matches, page.first, page.last), (0, 0, 0));
	assert!(page.repositories.is_empty());
}

#[rstest]
#[case(0, "0 repositories")]
#[case(1, "1 repository")]
#[case(265, "265 repositories")]
fn counts_read_naturally(#[case] count: usize, #[case] expected: &str) {
	assert_eq!(repository_count(count), expected);
}
