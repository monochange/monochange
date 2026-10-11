---
monochange_app: breaking
---

# Remove the placeholder roadmap and feedback server functions

**Breaking change:** the placeholder server functions that predate project feedback are removed from `monochange_app`, along with their HTTP endpoints:

- `server_fns::roadmap::list_roadmap`, `server_fns::roadmap::vote_roadmap_item`, and `server_fns::roadmap::RoadmapItem` (the whole `server_fns::roadmap` module);
- `server_fns::feedback::submit_feedback` and `server_fns::feedback::FeedbackSubmission`.

They returned empty results and nothing called them. Use the project feedback server functions instead: `portal_view`, `share_feedback`, and `portal_vote` in `server_fns::portal` for the public portal, and `feedback_console`, `feedback_action`, and `maintainer_reply` in `server_fns::feedback` for maintainers.

```rust
// before
let items = server_fns::roadmap::list_roadmap(repo_id).await?;

// after
let snapshot = server_fns::portal::portal_view(organization, project).await?;
let roadmap = snapshot.map(|snapshot| snapshot.feed.roadmap);
```

No migration guide applies: these functions were only reachable from the website's own pages, which the same release replaces.
