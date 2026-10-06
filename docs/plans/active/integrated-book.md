# Integrated book and simpler website examples

## Outcome

Read the monochange book at `/book` without leaving the website's navigation, typography, or theme. Keep the book's existing Markdown as the source of truth. Show short built-in commands and recognizable ecosystem icons.

## Design

- Embed the checked-in book at application build time. Read its table of contents rather than maintain a second chapter list.
- Render Markdown on the server, sanitize HTML, add stable heading anchors, and highlight fenced code blocks.
- Rewrite chapter and asset links for the integrated book. Preserve external links and offer an edit link to the original source.
- Use the existing website shell with chapter navigation, previous/next links, and responsive layouts.
- Keep the standalone mdBook usable with a visible link back to monochange.dev.
- Vendor a small pinned set of Simple Icons, with their source and license, rather than load an icon font or depend on a CDN.
- Use `monochange preview` in introductory examples. Keep precise step commands in the step reference.

## Checklist

- [x] Add fixture-backed Markdown, navigation, link, anchor, and highlighting tests.
- [x] Add compiled book rendering and the public routes.
- [x] Update navigation, footer, command examples, and ecosystem icons.
- [ ] Run required checks and inspect desktop and mobile behavior.
- [ ] Open a signed follow-up PR and merge through the normal queue.
- [ ] Verify the production book after an approved website release.

## Feature-request gateway

Track the gateway as a separate product change in [the product plan](feature-request-gateway.md). The proposed first version lets a user describe a request, lets a maintainer accept or decline it, and shows whether accepted work is planned, in progress, under review, or released. Maintainers bring their own agent; monochange supplies a handoff and records evidence from the linked issue, PR, and release. This PR records the plan; it does not expose the existing unimplemented feedback endpoints as a working request board.
