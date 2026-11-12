---
name: monochange
description: Release floor — mist/graphite grounds, teal signal, rounded display lettering, readable release plans.
colors:
  brand-50: "#f0fdfa"
  brand-100: "#ccfbf1"
  brand-200: "#99f6e4"
  brand-300: "#5eead4"
  brand-400: "#2dd4bf"
  brand-500: "#14b8a6"
  brand-600: "#0d9488"
  brand-700: "#0f766e"
  brand-800: "#115e59"
  brand-900: "#134e4a"
  brand-950: "#042f2e"
  page-light: "#E8EEF4"
  page-dark: "#0B141F"
  surface-light: "#ffffff"
  surface-dark: "#152536"
  muted-light: "#5A6B7D"
  muted-dark: "#9AADB8"
  rule-light: "#C5D0DC"
  rule-dark: "#2A3A4D"
  hero: "#071018"
  signal: "#E4B84A"
  danger-light: "#b4233f"
  danger-dark: "#fda4af"
  mc-accent: "#0D9488"
  mc-accent-deep: "#0F766E"
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
    backgroundColor: "{colors.brand-600}"
    textColor: "{colors.surface-light}"
    typography: "{typography.button-label}"
    rounded: "{rounded.control}"
    padding: ".85rem 1.35rem"
  button-brand-hover:
    backgroundColor: "{colors.brand-800}"
  button-light:
    backgroundColor: "{colors.brand-50}"
    textColor: "{colors.brand-900}"
    typography: "{typography.button-label}"
    rounded: "{rounded.control}"
    padding: ".85rem 1.35rem"
  button-light-hover:
    backgroundColor: "{colors.brand-200}"
  button-outline:
    textColor: "{colors.brand-50}"
    typography: "{typography.button-label}"
    rounded: "{rounded.control}"
    padding: ".85rem 1.35rem"
  button-outline-hover:
    backgroundColor: "{colors.brand-800}"
  strategy-selected:
    backgroundColor: "{colors.brand-100}"
    textColor: "{colors.brand-700}"
    typography: "{typography.label}"
    rounded: "{rounded.strategy}"
    padding: ".7rem .9rem"
  release-manifest:
    backgroundColor: "{colors.surface-light}"
    textColor: "{colors.brand-950}"
    rounded: "{rounded.panel}"
    padding: "{spacing.panel}"
  availability-note:
    rounded: "{rounded.control}"
    padding: "{spacing.inset}"
  command-block:
    typography: "{typography.code}"
    rounded: "{rounded.control}"
    padding: "{spacing.inset}"
---

# Design System: monochange

## Overview

**Creative North Star: "Shipping manifest"**

The flowing three-part mark and exact custom SVG wordmark identify monochange. Rounded display lettering gives headings their character. Package names, versions, commands, and repository rows carry the product detail in a readable layout with clear rules between records.

Light pages use a cool mist ground (`--mc-page`). Dark pages use graphite ink. Teal accents (`--mc-*`) connect both themes; the flowing mark may keep its violet lockup while UI chrome uses teal. Panels have modest curves; controls use bold text and visible keyboard focus.

**Landing and marketing chrome** compose layout and color with Tailwind utilities mapped through `@theme` (`bg-hero`, `text-mute`, `font-display`, `bg-brand-700`, …). Do not add new semantic CSS class catalogs for those surfaces; shared `--mc-*` custom properties remain for tokens and legacy pages.

This document records the implemented app in `crates/monochange_app/style/input.css` and its Rust components. The frontmatter contains the reusable values. `.impeccable/design.json` adds component previews, motion, breakpoints, and notes scoped to the public pages. The published book has its own reading layout.

**Key Characteristics:**

- The selected SVG mark and custom lettering remain the identity.
- Bricolage Grotesque headings pair with Hanken Grotesk reading text and controls.
- Color fields establish sections; ruled rows make release data easy to compare.
- Theme, keyboard, touch, and reduced-motion behavior are part of each component.

## Colors

The mark may keep its violet lockup; teal establishes interactive emphasis; mist or graphite grounds support reading.

### Primary

- `brand-500` and `brand-600` form the selected mark's violet-to-indigo gradient. `brand-600` also fills primary buttons and supplies the light-theme accent.
- `brand-800` fills the public hero and free-pricing section. It also supplies the primary-button hover color.
- `brand-700` emphasizes selected version controls and the manifest's next versions. `brand-900` is the text on light buttons.
- `brand-300` supplies the dark-theme accent, outline-button borders, and links in the dark walkthrough. `brand-200` supplies secondary text and focus outlines inside indigo sections.

### Neutral

- `brand-50` is the light page ground and dark page foreground. `brand-950` is the light page foreground, dark page ground, and walkthrough background.
- `surface-light` and `surface-dark` fill theme-aware panels. The example manifest always uses `surface-light` with `brand-950` text, including in dark mode.
- `muted-light` and `muted-dark` carry secondary text. `rule-light` and `rule-dark` separate sections and rows. `rule-manifest` is the fixed separator inside the white manifest.
- `brand-100` fills the selected manifest control. `brand-200` fills light-button hover states.

**The Theme Pairing Rule.** Prefer `--mc-*` tokens (`--mc-page`, `--mc-surface`, `--mc-ink`, `--mc-muted`, `--mc-rule`, `--mc-accent`, `--mc-accent-deep`). Legacy `--page` / `--ink` / `--accent` aliases map to the same values during migration.

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

Buttons have a minimum height of `48px`, centered content, bold text, and the recorded padding and curve. Primary buttons use the brand variant; light and outlined buttons sit on the indigo sections. The outlined variant has a one-pixel `brand-300` border. All variants retain their respective hover colors.

Keyboard focus uses a three-pixel accent outline with a four-pixel offset. Indigo sections change that outline to `brand-200`. Text actions use underlines, bold text, and vertical padding. Decorative arrow SVGs inherit the action color and do not supply its accessible name.

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

### Panels and commands

The login panel uses the theme-aware panel background, the panel curve, and `2.5rem` padding. On narrow screens it uses `2rem 1.5rem`. Its GitHub action appears only when authentication has a usable URL; unavailable sign-in has an explicit notice. The app uses GitHub sign-in rather than a styled text-input form.

The GitHub App availability notice uses the theme-aware panel background, the control curve, and the inset padding. Installation commands have an ink background and lavender text. The dark walkthrough command uses a border against the section background. Both preserve whitespace and allow local horizontal scrolling.

### Workspace and status content

Repository lists use ruled rows, a bold repository name, secondary access and installation details, and a free label. Loading, session failure, repository failure, and empty states use direct messages and the relevant sign-in, book, or setup action. Green and red authentication callback utilities are local status treatments, not additional brand accents.

Status chips use the soft accent fill with `mc-accent-deep` text, and `mc-accent` text in the dark theme, so they stay legible in both themes.

**The Danger Rule.** Destructive actions use one red pair, not the teal accent. `danger-light` (`#b4233f`) fills destructive buttons and colors the danger-zone toggle on light grounds; its hover fill is `#8b1029`. In the dark theme the danger-zone toggle switches to `danger-dark` (`#fda4af`), which keeps contrast on the graphite ground where the light red would sink. The toggle reads as destructive before it opens, and the confirming button stays the filled `danger-light` red in both themes.

### Feedback portal

The public portal at `/p/{org}/{project}` opens with an indigo hero: the project name, its description, four counts (under review, planned, in progress, shipped) in a ruled strip, and the visitor's GitHub identity or a light **Sign in with GitHub** button. Two violet radial glows sit behind the hero; they are decoration, so the hero still reads on the flat `brand-800`/`brand-900` field.

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
