# monochange Logo Gallery

Logo concepts for the monochange brand. Each logo is available in both SVG and PNG (512×512) formats.

## Design criteria

All logos were designed and reviewed against these strict criteria:

1. **Monochrome-first** — works in pure black on white; color is optional enhancement
2. **Favicon test** — icon marks must be recognizable at 32×32 pixels
3. **Symmetry/balance** — horizontally symmetrical or visually balanced
4. **Bold lines** — minimum 4px stroke at 512×512; text uses font-weight 900
5. **Simplicity** — maximum 3-4 visual elements
6. **Professional quality** — distinctive, not generic
7. **WCAG AA contrast** — all color palettes pass 4.5:1 contrast ratio

## Color palettes

### Monochrome (base set)

| Color | Hex       | Usage                            |
| ----- | --------- | -------------------------------- |
| Dark  | `#1a1a2e` | Primary dark background and text |
| Gray  | `#6b7280` | Secondary text                   |
| White | `#ffffff` | Text on dark backgrounds         |

### Emerald (recommended for production)

| Color    | Hex       | Usage          |
| -------- | --------- | -------------- |
| Emerald  | `#028A0F` | Primary accent |
| Navy     | `#0A2540` | Background     |
| Contrast | 4.50:1    | WCAG AA pass   |

Professional, trustworthy. Green communicates "go/ship/release." Best for a release-planning tool.

### Purple

| Color      | Hex       | Usage          |
| ---------- | --------- | -------------- |
| Purple     | `#8338ec` | Primary accent |
| Rich Black | `#040406` | Background     |
| Contrast   | 4.61:1    | WCAG AA pass   |

Modern and distinctive. Good for differentiation from competitors.

### Neon

| Color        | Hex       | Usage          |
| ------------ | --------- | -------------- |
| Neon Green   | `#39FF14` | Primary accent |
| Almost Black | `#070D0d` | Background     |
| Contrast     | 13.79:1   | WCAG AAA pass  |

Highest contrast. Best for dark-mode highlights and developer-facing contexts.

### Teal

| Color     | Hex       | Usage          |
| --------- | --------- | -------------- |
| Teal      | `#5FC9C0` | Primary accent |
| Deep Blue | `#090979` | Background     |
| Contrast  | 4.52:1    | WCAG AA pass   |

Calm, technical. Good for documentation and secondary contexts.

## Base logo concepts (monochrome)

### MC lettermarks (1–5)

| # | File                       | Description                                                                      |
| - | -------------------------- | -------------------------------------------------------------------------------- |
| 1 | `01-mc-bold`               | Bold "MC" on dark rounded square. The simplest, strongest icon mark.             |
| 2 | `02-mc-circle`             | "MC" on dark circle with subtle underline accent. Softer shape language.         |
| 3 | `03-mc-accent-bar`         | "MC" on dark rounded square with accent bar beneath. Clean with subtle emphasis. |
| 4 | `04-mc-monochrome-light`   | "MC" on white with bold dark border. Light-mode variant.                         |
| 5 | `05-mc-monochrome-tagline` | "MC" with underline and "MONOCHANGE" tagline. Full brand lockup.                 |

### monochange wordmarks (6–10)

| #  | File                    | Description                                                                     |
| -- | ----------------------- | ------------------------------------------------------------------------------- |
| 6  | `06-wordmark-bold`      | "monochange" in bold dark with underline. Clean wordmark for light backgrounds. |
| 7  | `07-wordmark-dark`      | "monochange" in white on dark rounded rectangle. Dark-mode wordmark.            |
| 8  | `08-wordmark-split`     | "mono" in bold dark + "change" in medium gray. Weight differentiation.          |
| 9  | `09-wordmark-uppercase` | "MONOCHANGE" uppercase with letterspacing and subtitle. Authoritative lockup.   |
| 10 | `10-wordmark-accent`    | "mono" bold + "change" gray with centered underline. Balanced variant.          |

### Combined marks (11–15)

| #  | File                     | Description                                                          |
| -- | ------------------------ | -------------------------------------------------------------------- |
| 11 | `11-mc-block-wordmark`   | Dark bar with "MC" block + "monochange" wordmark. Horizontal lockup. |
| 12 | `12-mc-name-dark`        | "MC" on dark square with subtle underline. Clean app icon.           |
| 13 | `13-large-m-wordmark`    | Large "M" on white with bold border. Letterform-focused mark.        |
| 14 | `14-wordmark-accent-bar` | Dark accent bar left + "monochange" wordmark. Minimal typographic.   |
| 15 | `15-hex-mc-wordmark`     | Hexagon with "MC" + "monochange" wordmark. Geometric icon pairing.   |

### Abstract marks (16–20)

| #  | File                    | Description                                                          |
| -- | ----------------------- | -------------------------------------------------------------------- |
| 16 | `16-stacked-bars`       | Three horizontal bars, opacity gradient. Monorepo packages.          |
| 17 | `17-concentric-squares` | Concentric rounded squares with solid center. Version containment.   |
| 18 | `18-delta-mc`           | Bold triangle with "MC" inside. Change/evolution symbol.             |
| 19 | `19-infinity-loop`      | Clean infinity symbol on white. Continuous release cycle.            |
| 20 | `20-hub-network`        | Central node with three satellites and lines. Monorepo coordination. |

## Color variants

Each icon mark has 4 color palette variants. Filenames follow the pattern `{base}-{palette}.svg`:

| Palette | Suffix     | Example                  |
| ------- | ---------- | ------------------------ |
| Emerald | `-emerald` | `01-mc-bold-emerald.svg` |
| Purple  | `-purple`  | `01-mc-bold-purple.svg`  |
| Neon    | `-neon`    | `01-mc-bold-neon.svg`    |
| Teal    | `-teal`    | `01-mc-bold-teal.svg`    |

Color variants exist for these base logos: 01, 02, 07, 11, 12, 16, 17, 18, 20.

## Recommendations

- **App icon / favicon**: `01-mc-bold` or `12-mc-name-dark` (monochrome or emerald)
- **Header / website**: `06-wordmark-bold` or `09-wordmark-uppercase`
- **Dark mode**: `07-wordmark-dark-emerald` or `11-mc-block-wordmark-emerald`
- **Documentation**: `05-mc-monochrome-tagline` or `10-wordmark-accent`
- **Feature illustrations**: `16-stacked-bars-emerald`, `17-concentric-squares-emerald`, or `20-hub-network-emerald`
- **Developer-facing**: `01-mc-bold-neon` or `12-mc-name-dark-neon`
