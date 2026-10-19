---
monochange_app: feat
---

# Filter and paginate repository lists

The new `monochange_app::repository_filter` module holds the dashboard's list rules: `RepositoryFilter` reads the `q` (name search) and `org` (account) query keys and builds links with `href`, `matches_search` matches a repository's name (or `owner/name` when the search contains `/`), and `repository_page` returns one `REPOSITORIES_PER_PAGE` (20) page with its position and page count.

The new `components::repository_list` module renders them: `RepositoryFilters` (a GET search form that updates the URL in place after hydration), `RepositoryPager` (one list's pages), and `RepositoryPicker` (project-form checkboxes with a name filter that only hides rows, so hidden selections still submit). `RepoInfo` now derives `PartialEq` and `Eq`.

```text
/dashboard?q=web&org=acme
```
