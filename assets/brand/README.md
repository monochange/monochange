# monochange identity

The selected identity is the flowing three-part mark with custom lowercase lettering. The shapes were traced from Ifiok's selected artwork into SVG paths, preserving the original silhouette and lettering without requiring a font. The colour treatment was explored with the built-in image-generation tool, then applied to the SVG sources for clean edges and consistent exports.

| Asset                                | Purpose                                                               |
| ------------------------------------ | --------------------------------------------------------------------- |
| `mark.svg`                           | Transparent square mark for navigation and compact placements         |
| `wordmark.svg` / `wordmark-dark.svg` | Approved custom lettering for horizontal navigation lockups           |
| `avatar.svg`                         | Square mark on pale lavender for organization and application avatars |
| `avatar-dark.svg`                    | Square mark on ink for dark surfaces                                  |
| `logo.svg` / `logo-dark.svg`         | Stacked mark and wordmark on transparent backgrounds                  |
| `social.svg`                         | Source for the website's 1200 × 630 sharing card                      |

The mark runs from violet `#8b5cf6` to indigo `#4f46e5`. Lettering is ink `#19152e` on light surfaces and pale lavender `#f5f3ff` on dark surfaces. Preserve the aspect ratio, negative-space cuts, and padding. Use the mark alone at small sizes; the wordmark is for larger placements.

`assets/logo-280.png` and its dark variant are the README lockups. The 512 and 1024 exports are square avatars; existing Rust documentation URLs use the 512 export and `assets/favicon.ico`. Documentation copies and website icons are exported from the same SVG sources. The README picture is shared through `.templates/branding.t.md` and synchronized with `docs:update`.

Colour-edit prompt used with the built-in tool: preserve exactly the selected mark's three rounded flowing segments, silhouette curves, negative-space cuts, and custom lowercase monochange lettering; change only the colour to violet-to-indigo and ink lettering, with a transparent background. The square version removes lettering, preserves the same mark, centers it with balanced padding, and uses a pale lavender background.

For raster exports, render the SVG at the target dimensions. For example:

```sh
magick -background none assets/brand/logo.svg -resize 280x assets/logo-280.png
magick assets/logo-512.png -define icon:auto-resize=48,32,16 assets/favicon.ico
```

Keep the copies under `docs/src/branding` and `app/public/branding` synchronized when changing these source assets. Legacy gallery concepts remain in `assets/reserve` and `app/public/branding/logos`; they are not the active identity.
