---
name: monochange
description: Ultraviolet release floor — lavender and aubergine grounds, the mark’s violet as the accent, saffron signal, and a warming bump scale that reads before its labels.
colors:
  brand-50: "#f6f4ff"
  brand-100: "#ebe8ff"
  brand-200: "#d8d3fe"
  brand-300: "#beb7fe"
  brand-400: "#9f93ff"
  brand-500: "#8167ff"
  brand-600: "#6943ee"
  brand-700: "#5433d1"
  brand-800: "#4128ac"
  brand-900: "#302486"
  brand-950: "#1b1655"
  page-light: "#f1effb"
  page-dark: "#120f24"
  surface-light: "#ffffff"
  surface-dark: "#1f1a36"
  ink-light: "#19152e"
  ink-dark: "#f3f2fc"
  muted-light: "#625f7e"
  muted-dark: "#b4b0cf"
  rule-light: "#d5d3e9"
  rule-dark: "#383456"
  accent-soft-light: "#e6e2fe"
  accent-soft-dark: "#302661"
  hero: "#190c42"
  hero-fg: "#f3f2fc"
  hero-muted: "#bdb9dc"
  mc-accent: "#6943ee"
  mc-accent-deep: "#5433d1"
  mc-accent-dark: "#9f93ff"
  mc-accent-deep-dark: "#8167ff"
  signal: "#ffc641"
  flare: "#ff906d"
  mint: "#62e5b5"
  signal-deep: "#8d5403"
  signal-ink: "#663d07"
  signal-soft: "#feefc7"
  signal-soft-dark: "#462d0b"
  signal-text-dark: "#fbe08a"
  signal-dot: "#d08506"
  mint-deep: "#066f51"
  mint-soft: "#cef8e5"
  mint-soft-dark: "#06402e"
  mint-text-dark: "#a0eecd"
  mint-dot: "#049e74"
  declined-dot: "#8885a0"
  danger-light: "#b61537"
  danger-hover: "#900d2a"
  danger-soft: "#ffe3e6"
  danger-dark: "#fca0a7"
typography:
  display:
    fontFamily: "Bricolage Grotesque, sans-serif"
    fontSize: "clamp(3.1rem, 4.6vw, 4.75rem)"
    fontWeight: 800
    lineHeight: 1.06
    letterSpacing: "-0.035em"
  display-mobile:
    fontFamily: "Bricolage Grotesque, sans-serif"
    fontSize: "clamp(2.4rem, 9vw, 3.6rem)"
    fontWeight: 800
    lineHeight: 1.06
    letterSpacing: "-0.035em"
  page-title:
    fontFamily: "Bricolage Grotesque, sans-serif"
    fontSize: "clamp(2.8rem, 4.5vw, 4.5rem)"
    fontWeight: 800
    lineHeight: 1.06
    letterSpacing: "-0.035em"
  headline:
    fontFamily: "Bricolage Grotesque, sans-serif"
    fontSize: "clamp(2.4rem, 3.2vw, 3.4rem)"
    fontWeight: 800
    lineHeight: 1.06
    letterSpacing: "-0.035em"
  title:
    fontFamily: "Bricolage Grotesque, sans-serif"
    fontSize: "2rem"
    fontWeight: 800
    lineHeight: 1.06
    letterSpacing: "-0.035em"
  workspace-title:
    fontFamily: "Bricolage Grotesque, sans-serif"
    fontSize: "clamp(2rem, 3vw, 2.75rem)"
    fontWeight: 800
    lineHeight: 1.1
    letterSpacing: "-0.035em"
  workspace-section:
    fontFamily: "Bricolage Grotesque, sans-serif"
    fontSize: "clamp(1.5rem, 2vw, 1.75rem)"
    fontWeight: 800
    lineHeight: 1.1
    letterSpacing: "-0.035em"
  body:
    fontFamily: "Hanken Grotesk, sans-serif"
    fontWeight: 400
    lineHeight: 1.6
  label:
    fontFamily: "Hanken Grotesk, sans-serif"
    fontSize: ".9rem"
    fontWeight: 700
    lineHeight: 1.6
  button-label:
    fontFamily: "Hanken Grotesk, sans-serif"
    fontWeight: 700
    lineHeight: 1.4
  code:
    fontFamily: 'ui-monospace, "SFMono-Regular", Consolas, monospace'
    fontSize: ".9rem"
    fontWeight: 400
    lineHeight: 1.6
rounded:
  strategy: "8px"
  navigation-control: "10px"
  control: "12px"
  panel: "16px"
spacing:
  action-gap: "1rem"
  inset: "1.5rem"
  panel: "2rem"
  panel-large: "2.5rem"
  layout-gap: "4rem"
  section: "5rem"
  section-large: "6.5rem"
components:
  button-brand:
    backgroundColor: "{colors.brand-700}"
    textColor: "{colors.surface-light}"
    typography: "{typography.button-label}"
    rounded: "{rounded.control}"
    padding: ".85rem 1.35rem"
  button-brand-hover:
    backgroundColor: "{colors.brand-800}"
  button-light:
    backgroundColor: "{colors.brand-100}"
    textColor: "{colors.brand-700}"
    typography: "{typography.button-label}"
    rounded: "{rounded.control}"
    padding: ".85rem 1.35rem"
  button-light-hover:
    backgroundColor: "{colors.brand-200}"
  button-outline:
    textColor: "{colors.hero-fg}"
    typography: "{typography.button-label}"
    rounded: "{rounded.control}"
    padding: ".85rem 1.35rem"
  button-outline-hover:
    backgroundColor: "{colors.brand-600}"
  strategy-selected:
    backgroundColor: "{colors.brand-100}"
    textColor: "{colors.brand-700}"
    typography: "{typography.label}"
    rounded: "{rounded.strategy}"
    padding: ".7rem .9rem"
  release-manifest:
    backgroundColor: "{colors.surface-light}"
    textColor: "{colors.ink-light}"
    rounded: "{rounded.panel}"
    padding: "{spacing.panel}"
  availability-note:
    rounded: "{rounded.control}"
    padding: "{spacing.inset}"
  command-block:
    typography: "{typography.code}"
    rounded: "{rounded.control}"
    padding: "{spacing.inset}"
  console-patch:
    backgroundColor: "{colors.mint}"
    textColor: "{colors.hero}"
  console-minor:
    backgroundColor: "{colors.signal}"
    textColor: "{colors.hero}"
  console-major:
    backgroundColor: "{colors.flare}"
    textColor: "{colors.hero}"
  console-dependency:
    textColor: "{colors.brand-300}"
---

# Design System: monochange

## Overview

**Creative North Star: "Shipping manifest"**

The flowing three-part mark and exact custom SVG wordmark identify monochange. Rounded display lettering gives headings their character. Package names, versions, commands, and repository rows carry the product detail in a readable layout with clear rules between records.

Light pages use a lavender paper ground (`--mc-page`) with aubergine ink, the same ink the wordmark is drawn in. Dark pages use a deep aubergine night. The interface accent is the mark’s own violet, so the logo, the browser icons and every link, button and focus ring speak one colour. The hero band is a saturated ultraviolet, not black. Saffron is the one warm signal, and the release console adds a small warming scale for bump levels. Panels have modest curves; controls use bold text and visible keyboard focus.

**Landing and marketing chrome** compose layout and color with Tailwind utilities mapped through `@theme` (`bg-hero`, `text-mute`, `font-display`, `bg-brand-700`, …). Do not add new semantic CSS class catalogs for those surfaces; shared `--mc-*` custom properties remain for tokens and legacy pages.

This document records the implemented app in `crates/monochange_app/style/input.css` and its Rust components. The frontmatter contains the reusable values. `.impeccable/design.json` adds component previews, motion, breakpoints, and notes scoped to the public pages. The published book has its own reading layout.

**Key Characteristics:**

- The selected SVG mark and custom lettering remain the identity.
- Bricolage Grotesque headings pair with Hanken Grotesk reading text and controls.
- Color fields establish sections; ruled rows make release data easy to compare.
- Theme, keyboard, touch, and reduced-motion behavior are part of each component.

## Colors

**Ultraviolet.** monochange is one hue. The mark, the wordmark ink, the browser icons and the interface accent all come from the same violet, and the neutrals are tinted towards it, so the interface and the logo finally read as one brand. Colour other than violet means something: saffron is the signal, and the release console warms from mint to saffron to flare as a change grows. Nothing else gets a hue.

Every value was chosen in OKLCH so the steps are perceptually even across both themes, then stored as the sRGB hex it maps to. The brand ramp holds lightness on a steady staircase (`0.972` at `brand-50` down to `0.255` at `brand-950`), peaks in chroma at `brand-600` (`0.24`), and drifts in hue from lavender (`293°`) to indigo (`279°`) the way the mark's gradient does. Light tints are clipped to the sRGB gamut, so they stay soft instead of fluorescent.

### Primary

- `brand-600` (`oklch(0.535 0.24 285)`) is the light-theme accent (`--mc-accent`): links, current navigation, focus rings, step numerals and ecosystem icons. `brand-700` (`--mc-accent-deep`) fills primary buttons, colours chip text on the soft accent, and is the browser `theme-color`; `brand-800` is its hover.
- `brand-400` (`oklch(0.72 0.175 287)`) is the dark-theme accent and the accent on the hero, including the hero navigation. `brand-500` is the dark theme's deep accent.
- `brand-300` colours the changeset package name and dependency rows in the console, borders the outlined hero button, and fills the pressed package choice with `brand-950` text.
- `brand-100` fills the hero's light button and the current-page navigation pill (`brand-700` or `brand-800` text); `brand-200` is its hover. In the dark theme the current-page pill is `brand-900` with `brand-100` text.

### Neutral

- Light: `page-light` (`#f1effb`, lavender paper), `surface-light` (white panels), `ink-light` (`#19152e`, the wordmark's aubergine), `muted-light` (`#625f7e`) and `rule-light` (`#d5d3e9`).
- Dark: `page-dark` (`#120f24`, aubergine night), `surface-dark` (`#1f1a36`), `ink-dark` (`#f3f2fc`), `muted-dark` (`#b4b0cf`) and `rule-dark` (`#383456`).
- `accent-soft-light` and `accent-soft-dark` fill status chips, selection and the mark tiles on the login and pricing pages.
- `hero` (`#190c42`, `oklch(0.215 0.095 286)`) is the ultraviolet band behind the home hero, the pricing statement, the portal hero and every command block, in both themes. `hero-fg` and `hero-muted` are its text. The light theme's workspace band uses `ink-light` instead; the dark theme uses `hero`.

### Semantic hues

Four hues sit away from violet so they never read as the accent. Each has a bright step for the hero and dark grounds and a deep step with a soft fill for light grounds.

| Hue                | Bright (hero, dark) | Deep on soft (light)                   | Roles                                                         |
| ------------------ | ------------------- | -------------------------------------- | ------------------------------------------------------------- |
| Mint               | `#62e5b5`           | `#066f51` on `#cef8e5`                 | Patch bump, success, shipped                                  |
| Saffron (`signal`) | `#ffc641`           | `#8d5403` on `#feefc7`                 | Minor bump, the hero eyebrow and live dot, caution and voting |
| Flare              | `#ff906d`           | —                                      | Major bump only                                               |
| Danger             | `#fca0a7`           | `#900d2a` on `#ffe3e6`; fill `#b61537` | Destructive actions and errors                                |

**The Bump Scale Rule.** In the home console a bump level owns its hue everywhere it appears: the word in the changeset, the pressed bump choice, the row's tint and left rule, the next version and the status tag. Patch is mint, minor is saffron, major is flare, so the scale runs cooler to hotter as the change grows. A dependency row keeps the brand violet (`brand-300` text, `brand-500` rule), because it is monochange that pulled it into the release; unchanged rows stay `hero-muted`. Labels always accompany the colour. Flare is reserved for major bumps and is never used for errors; danger stays the separate crimson pair.

**The Theme Pairing Rule.** Prefer `--mc-*` tokens (`--mc-page`, `--mc-surface`, `--mc-ink`, `--mc-muted`, `--mc-rule`, `--mc-accent`, `--mc-accent-deep`, `--mc-accent-soft`). Legacy `--page` / `--ink` / `--accent` aliases map to the same values during migration. A fill that carries text in both themes gets an explicit text colour per theme; in the dark theme, text on an accent fill is `--mc-page`, never `--mc-ink`.

### Contrast

WCAG 2 ratios for the key pairs, measured on the stored hex values. Console rows are measured on their tint composited over the hero.

| Foreground               | Background                    | Use                              | Ratio   |
| ------------------------ | ----------------------------- | -------------------------------- | ------- |
| `ink-light` `#19152e`    | `page-light` `#f1effb`        | Body text, light                 | 15.55:1 |
| `muted-light` `#625f7e`  | `page-light` `#f1effb`        | Secondary text, light            | 5.34:1  |
| `muted-light` `#625f7e`  | `accent-soft-light` `#e6e2fe` | Secondary text on a chip         | 4.82:1  |
| `brand-600` `#6943ee`    | `page-light` `#f1effb`        | Accent text, links, focus ring   | 5.08:1  |
| `brand-600` `#6943ee`    | `surface-light` `#ffffff`     | Accent text on panels            | 5.77:1  |
| `#ffffff`                | `brand-700` `#5433d1`         | Primary button label             | 7.58:1  |
| `brand-700` `#5433d1`    | `brand-100` `#ebe8ff`         | Light button, current navigation | 6.32:1  |
| `brand-700` `#5433d1`    | `accent-soft-light` `#e6e2fe` | Status chip, light               | 6.02:1  |
| `ink-dark` `#f3f2fc`     | `page-dark` `#120f24`         | Body text, dark                  | 16.90:1 |
| `muted-dark` `#b4b0cf`   | `surface-dark` `#1f1a36`      | Secondary text on panels, dark   | 7.98:1  |
| `brand-400` `#9f93ff`    | `page-dark` `#120f24`         | Accent text, focus ring, dark    | 7.23:1  |
| `brand-400` `#9f93ff`    | `accent-soft-dark` `#302661`  | Status chip, dark                | 5.14:1  |
| `brand-100` `#ebe8ff`    | `brand-900` `#302486`         | Current navigation, dark         | 10.17:1 |
| `hero-fg` `#f3f2fc`      | `hero` `#190c42`              | Hero headline                    | 16.13:1 |
| `hero-muted` `#bdb9dc`   | `hero` `#190c42`              | Hero secondary text              | 9.50:1  |
| `brand-400` `#9f93ff`    | `hero` `#190c42`              | Hero navigation                  | 6.90:1  |
| `brand-950` `#1b1655`    | `brand-300` `#beb7fe`         | Pressed package choice           | 8.82:1  |
| `signal` `#ffc641`       | minor row `#301f42`           | Minor bump in the console        | 9.57:1  |
| `flare` `#ff906d`        | major row `#351c47`           | Major bump in the console        | 6.72:1  |
| `mint` `#62e5b5`         | patch row `#20224e`           | Patch bump in the console        | 9.57:1  |
| `brand-300` `#beb7fe`    | dependency row `#20134b`      | Dependency in the console        | 9.07:1  |
| `hero` `#190c42`         | `flare` `#ff906d`             | Pressed major choice             | 8.07:1  |
| `#ffffff`                | `danger-light` `#b61537`      | Destructive button               | 6.66:1  |
| `danger-hover` `#900d2a` | `#ffe3e6`                     | Form error                       | 7.63:1  |
| `danger-dark` `#fca0a7`  | `page-dark` `#120f24`         | Danger-zone toggle, dark         | 9.58:1  |
| `#066f51`                | `#cef8e5`                     | Shipped badge, light             | 5.35:1  |
| `#8d5403`                | `#feefc7`                     | Voting badge, light              | 5.40:1  |

## Typography

Display and first-level section headings use self-hosted Bricolage Grotesque 800. Body text, third-level headings, and controls use self-hosted Hanken Grotesk 400 or 700. The font files live in `public/fonts/`. Commands use the system monospace stack.

### Hierarchy

- `display` is the desktop home heading. At the intermediate breakpoint it becomes a fixed `3.3rem`; `display-mobile` takes over at the narrow breakpoint.
- `page-title` is the installation and login introduction. `headline` is the large section-heading style. `title` is the smaller installation section-heading style; the error page title uses the `headline` step.
- `workspace-title` is the one title scale for the projects pages: the workspace landing, organization, project and account headings. It stays below the marketing display sizes so a page reads as a place to work, and long identifiers such as generated organization logins wrap anywhere instead of filling the viewport. `workspace-section` heads the sections beneath it (repositories, the workspace guide); an organization's heading on the workspace landing steps down once more to `clamp(1.35rem, 2vw, 1.6rem)`, and project and repository card names sit at `1.2rem` and `1.05rem` in the body face.
- `body` inherits the browser's base font size. Public explanatory paragraphs use local sizes between `1.05rem` and `1.3rem`, with widths between `40ch` and `65ch`.
- `label` describes the manifest strategy controls. `button-label` leaves size inherited and uses a tighter line height. There is no uppercase label convention.
- `code` keeps command examples compact. Version tables use tabular numerals. Headings balance their lines.

**The Wordmark Rule.** Render the exact lettering in `public/branding/wordmark.svg` or `public/branding/wordmark-dark.svg`. A font-rendered product name is not a replacement for these assets.

## Layout

The shared container is centered and capped at `1280px`. Its default width is `calc(100% - 6rem)`. At `1050px` and below it becomes `calc(100% - 4rem)`; at `760px` and below it becomes `calc(100% - 2.5rem)`. These leave `3rem`, `2rem`, and `1.25rem` on each side respectively.

The header stays at the top of the viewport. Its navigation row has a minimum height of `88px`, reduced to `76px` on narrow screens. Wide layouts generally pair two columns. Narrow layouts stack the home, section introduction, walkthrough, installation, GitHub App, login, and pricing content. The ecosystem list moves from three columns to two and retains two on mobile. The footer moves from three columns to one.

Action groups wrap with the recorded action gap. Ordinary page sections use generous vertical spacing, while workspace records use ruled rows with smaller insets. Commands and the manifest table allow local horizontal overflow. Long repository names wrap anywhere.

The public home layout has a checked `375 × 812` first viewport with readable npm and Cargo rows, including current and next versions. Retain that concrete content when changing the public heading or action spacing. This is a public-home constraint, not a layout requirement for every app page.

## Elevation & Depth

Color fields and borders provide most of the separation. The example manifest is the main raised panel, with `box-shadow: 0 24px 48px #19152e35`. The login panel and availability notice rely on their theme-aware background rather than an added shadow. The header is solid, sticky, and bordered, with no blur.

The manifest enters with a `12px` upward settling movement over `.7s`, using `cubic-bezier(.16,1,.3,1)`. Buttons change background and text color over `.18s ease`. Reduced-motion mode disables animation and transitions and restores automatic scrolling. Motion does not hide any release information.

## Shapes

Version strategy controls use the smallest recorded curve. Theme and menu controls use the navigation-control curve. Buttons, commands, and availability notices use the control curve. The release manifest and login panel use the panel curve. One-pixel rules separate content. Avatar images are circular.

The selected three-part mark uses flowing curved silhouettes. Use `public/branding/mark.svg` as the canonical vector. The wordmark is custom lettering; keep its proportions and use the dark asset when the page is dark.

## Components

### Buttons and links

Buttons have a minimum height of `48px`, centered content, bold text, and the recorded padding and curve. Primary buttons use the brand variant; light and outlined buttons sit on the ultraviolet hero band. The outlined variant has a one-pixel `brand-300` border. All variants retain their respective hover colors.

Keyboard focus uses a three-pixel accent outline with a four-pixel offset. The portal hero changes that outline to `brand-200`. Text actions use underlines, bold text, and vertical padding. Decorative arrow SVGs inherit the action color and do not supply its accessible name.

### Navigation

The home link pairs the selected mark with the appropriate exact wordmark. Desktop links show an accent hover and current-page state. At the narrow breakpoint, the desktop group and sign-in button give way to a menu control. The menu exposes `aria-expanded` and `aria-controls`, closes on Escape or a route change, and gives each row a minimum `44px` height.

Both browser entries render the same header, so the website and the projects workspace share one set of chrome. Signed out, the right-hand end holds the theme toggle and one filled sign-in button.

### Account menu

Signed in, the right-hand end holds the theme toggle, a **Projects** link, and one account menu in place of the separate buttons it replaces. The Projects link is the header's one filled action and takes the tinted current-page treatment while the visitor is inside the workspace.

The account menu is a `<details>` disclosure. On wide screens its toggle is a pill holding the GitHub avatar (the visitor's initial on a `brand-100` disc when there is no avatar) and a chevron, with the account named for screen readers only. The open menu is a `w-72` panel with the panel curve, a rule border and a soft shadow. It opens with "Signed in as" and the login, followed by **Connect repositories**, **Account and devices**, and **Sign out**. Every row is at least `44px` tall and tints on hover from its own text color, so the same row works in the dropdown and on the homepage's dark mobile panel.

The menu closes on Escape, on a route change, and on an outside click, with CSS only: the open menu draws a transparent full-viewport layer from its toggle. Focus returns to the toggle. In the mobile panel the toggle becomes a full-width row naming the account, the same rows open in place beneath it, and the Projects link closes the panel as a full-width button. Session refetches keep the current controls on screen, so the header never flashes between pages.

### Footer

One shared footer ends every public page and every projects page, so a visitor never notices which entry served the page. It has a hairline top rule and three columns on wide screens: the mark and wordmark above a one-line statement of what monochange is ("Release planning for monorepos."); the site links (Install, Read the book, Changelog, It's free) and GitHub; and the free-and-open-source line with its attribution. Below `760px` the columns stack into one. The tagline stays plain and describes the product, never repeating the homepage headline. Links between the two entries use document navigation, and only the website entry shows the deployed website version, linked to its changelog.

Theme and menu controls have minimum `44px` square hit areas. The theme button names the theme it will select. The app restores a stored preference after hydration, otherwise uses the system preference. The skip link appears on focus and targets `#main-content`. Header anchor clearance uses `7rem` scroll padding.

### Release manifest

This public-home component is a white panel with ruled package rows. Its padding shrinks to `1.4rem` at the intermediate breakpoint. The heading, ecosystem labels, and current/next versions remain separate. On narrow screens, row padding shrinks and version text stays unbroken.

The two strategy buttons use `aria-pressed` and minimum `44px` heights. Switching strategy changes example versions and explanatory text. The table has a descriptive caption, column and row headers, and a polite atomic live region. Keep the "Example workspace" label. This content demonstrates behavior and does not describe a visitor's connected packages.

### Release plan console

The homepage hero's console pairs a changeset with a plan table. Each row names a package, its current and next version, and a status tag (`minor`, `dependency`, and so on) beside the tinted, left-ruled row. At `760px` and below the table has no room for a status column, so the column gives way and the same tag sits under the package name; every row fits the panel without horizontal scrolling at `360px`. Only one copy of the tag is displayed at a time, so the status is read once. The package and bump buttons keep their markup at every width.

Colour carries the bump level (see **The Bump Scale Rule**): the changed row, its next version, its tag, the bump word in the changeset and the pressed bump choice all take mint, saffron or flare for patch, minor or major. The dependency row stays violet and unchanged rows stay muted, so the eye finds the change, then what it pulled along. The pressed package choice stays `brand-300`.

### Panels and commands

The login panel uses the theme-aware panel background, the panel curve, and `2.5rem` padding. On narrow screens it uses `2rem 1.5rem`. Its GitHub action appears only when authentication has a usable URL; unavailable sign-in has an explicit notice. The app uses GitHub sign-in rather than a styled text-input form.

The GitHub App availability notice uses the theme-aware panel background, the control curve, and the inset padding. Installation commands use the `hero` ground with `hero-fg` text in both themes; book code highlights keywords in `brand-400`, strings in mint, constants in saffron and names in flare. The dark walkthrough command uses a border against the section background. Both preserve whitespace and allow local horizontal scrolling.

### Workspace and status content

Repository lists use ruled rows, a bold repository name, secondary access and installation details, and a free label. Loading, session failure, repository failure, and empty states use direct messages and the relevant sign-in, book, or setup action. Green and red authentication callback utilities are local status treatments, not additional brand accents.

Status chips use the soft accent fill with `mc-accent-deep` text, and `mc-accent` text in the dark theme, so they stay legible in both themes.

**The Danger Rule.** Destructive actions use one crimson pair, never the violet accent and never flare. `danger-light` (`#b61537`) fills destructive buttons and colors the danger-zone toggle on light grounds; its hover fill is `danger-hover` (`#900d2a`), which is also the form-error text on `#ffe3e6`. In the dark theme the danger-zone toggle switches to `danger-dark` (`#fca0a7`), which keeps contrast on the aubergine ground where the light red would sink. The toggle reads as destructive before it opens, and the confirming button stays the filled `danger-light` red in both themes.

### Feedback portal

The public portal at `/p/{org}/{project}` opens with the ultraviolet hero: the project name, its description, four counts (under review, planned, in progress, shipped) in a ruled strip, and the visitor's GitHub identity or a light **Sign in with GitHub** button. Two violet radial glows sit behind the hero; they are decoration, so the hero still reads on the flat `hero` field. Roadmap status dots use the semantic hues: violet for planned and in progress, saffron for under review, mint for shipped and a muted violet-grey for declined.

Below it, the roadmap is a board of cards grouped by status, each group marked by a coloured dot and a count. Wide layouts put the roadmap on the left and the composer and "Yours" on the right; narrow layouts stack composer, roadmap, then "Yours". The vote control is a tall pill with a chevron and a tabular count; the pressed state fills with the brand gradient and uses `aria-pressed`. Signed-out visitors see the same control as a sign-in link.

Writing needs GitHub sign-in, so signed-out composers and discussions show a dashed invitation panel instead of a form; reading never does. A visitor's own items carry a five-step tracker (shared, reviewed, planned, building, shipped) with `aria-current="step"`, and declined or closed items show their status instead. Every action stays a plain form that works before hydration.

## Do's and Don'ts

### Do:

- Do use the exact selected mark and SVG lettering in their established proportions.
- Do pair theme-aware backgrounds with their corresponding foreground and border properties.
- Do preserve visible focus, the skip link, descriptive control labels, and the menu's keyboard behavior.
- Do keep public primary controls at least `44px` tall and preserve the existing `48px` button minimum.
- Do label example release data and keep package names, ecosystems, and versions readable on mobile.
- Do describe the CLI as available and free, link documentation to the published book, and state that hosted GitHub App installation is pending.

### Don't:

- Don't typeset a substitute wordmark or generate a new logo for an app component.
- Don't add paid tiers, checkout controls, usage figures, customer proof, or an available GitHub App installation claim.
- Don't turn the public-home composition into a requirement for the book or workspace dashboard.
- Don't rely on motion, hover, or color alone to communicate a control's selected state or a release version.
