/**
 * Design tokens shared by Acme products.
 *
 * Tokens are plain objects so consumers can tree-shake individual scales and
 * keep the runtime dependency-free.
 */

/** Surface, text, and accent colors. */
export const colors = {
	surface: "#ffffff",
	surfaceMuted: "#f4f5f7",
	text: "#1b1f24",
	textMuted: "#5b6570",
	accent: "#2f6fed",
	danger: "#c0392b",
} as const;

/** Spacing scale in pixels. */
export const spacing = {
	xs: 4,
	sm: 8,
	md: 16,
	lg: 24,
	xl: 40,
} as const;

/** Corner radii in pixels. */
export const radii = {
	sm: 4,
	md: 8,
	pill: 999,
} as const;

/** Type ramp used by the marketing and app surfaces. */
export const typography = {
	body: { family: "Inter, system-ui, sans-serif", size: 14, lineHeight: 1.5 },
	heading: { family: "Inter, system-ui, sans-serif", size: 24, lineHeight: 1.25 },
} as const;

/** Valid keys of the {@link colors} scale. */
export type ColorToken = keyof typeof colors;

/** Valid keys of the {@link spacing} scale. */
export type SpacingToken = keyof typeof spacing;

/** Valid keys of the {@link radii} scale. */
export type RadiusToken = keyof typeof radii;
