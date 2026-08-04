/**
 * Identity colors — the one palette for "which thing is this?"
 *
 * A thing that carries a color (a pin, a page, a chat, a notebook) shows it
 * wherever its NAME appears: the dot or glyph on its sidebar row, its tab icon,
 * the dot in the path mast. It never colors a container — a pane holds tabs
 * from many worlds, so painting the pane would claim the window for one of
 * them.
 *
 * There used to be TWO palettes here. `BOOKCLOTH` was six hard-coded hexes
 * (navy, cinder, cerulean, ochre, moss, damson) used for the automatic,
 * hash-derived color, while anything the user PICKED came from the nine
 * `--cat-*` tokens. So the dot a pin got for free was muted cloth and the dot
 * they chose was bright candy, in the same column, and neither could be
 * reached from the other. Worse, bookcloth was pure TypeScript: six literal
 * hexes that ignored all sixteen themes, which is exactly the failure the
 * token rule exists to prevent.
 *
 * Now there is one palette: `--cat-*`, nine hues, authored per theme with a
 * lightened set for the nine dark ones. The automatic color hashes into it and
 * a chosen color names one of its keys, so both ends of the same column speak
 * the same language.
 */

/** The nine, in spectrum order — this is also the picker's order. */
export const PIN_COLORS: { key: string; label: string }[] = [
	{ key: 'rose', label: 'Rose' },
	{ key: 'orange', label: 'Orange' },
	{ key: 'yellow', label: 'Yellow' },
	{ key: 'emerald', label: 'Emerald' },
	{ key: 'cyan', label: 'Cyan' },
	// Key stays `indigo` — it's the value stored in app_pins.color,
	// app_pages.icon_color and friends — but the hue is a true blue now, so
	// the label says what the user actually sees.
	{ key: 'indigo', label: 'Blue' },
	{ key: 'violet', label: 'Violet' },
	{ key: 'purple', label: 'Purple' },
	{ key: 'pink', label: 'Pink' },
];

const KEYS = PIN_COLORS.map((c) => c.key);

/**
 * The automatic color for a thing that hasn't chosen one: a stable hash of its
 * id into the same nine. No schema and no migration — the same pin gets the
 * same hue on every device — and because it lands on a token rather than a
 * literal, it adapts per theme like everything else.
 */
export function pinColor(id: string): string {
	let h = 0;
	for (let i = 0; i < id.length; i++) {
		h = (h * 31 + id.charCodeAt(i)) | 0;
	}
	return KEYS[Math.abs(h) % KEYS.length];
}

/* ────────────────────────────────────────────────────────────────────────────
 * Custom colors
 *
 * A picked token is theme-authored and needs no help. A custom hex is the
 * user's literal choice and would otherwise be the ONLY color in the app that
 * ignores the theme — `#14283D` chosen on Caladan is invisible on Borghese's
 * pure black, and a yellow picked on a dark theme screams on paper.
 *
 * So the hex is stored verbatim (their choice is the truth) and only its
 * LIGHTNESS is moved at render, into a band the theme guarantees is legible.
 * Hue and chroma — the part they actually chose — are untouched, in OKLab, so
 * the correction is perceptual rather than the sRGB smear that clamping HSL
 * would give (HSL calls a yellow and a navy the same lightness).
 *
 * Done in JS, not with `oklch(from …)` relative color, which needs Safari
 * 16.4 against declared floors of iOS 15 / macOS 10.15.
 * ──────────────────────────────────────────────────────────────────────────── */

/**
 * The legible band, per mode — and it is ONE-SIDED on purpose.
 *
 * On paper, a dark color is perfectly readable; what fails is a pale one, so
 * light themes only cap the top. On a dark surface it's the reverse: a bright
 * color is fine, a near-black one vanishes, so dark themes only raise the
 * floor. Clamping both ends would "correct" a deep navy on white into a washed
 * slate — legible before, less legible after, and no longer the color anyone
 * picked. The point is to rescue the cases that genuinely fail, not to make
 * every custom color match the nine.
 */
const BAND = {
	light: { min: 0, max: 0.72 },
	dark: { min: 0.6, max: 1 },
};

/**
 * Is the active theme a dark one? Read from `--identity-dark`, which the dark
 * themes set alongside their `--cat-*` overrides — so adding a theme is still
 * one block in themes.css, not an entry in a list over here that someone will
 * forget.
 *
 * Cached: this runs per rendered color, and `getComputedStyle` forces style
 * resolution. Invalidated by the same `themechange` event the rest of the app
 * already listens for.
 */
let darkCache: boolean | null = null;
if (typeof window !== 'undefined') {
	window.addEventListener('themechange', () => {
		darkCache = null;
	});
}

function isDarkTheme(): boolean {
	if (darkCache !== null) return darkCache;
	if (typeof document === 'undefined') return false;
	const flag = getComputedStyle(document.documentElement)
		.getPropertyValue('--identity-dark')
		.trim();
	darkCache = flag === '1';
	return darkCache;
}

const srgbToLinear = (c: number) =>
	c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
const linearToSrgb = (c: number) =>
	c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;

function hexToOklab(hex: string): [number, number, number] | null {
	const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
	if (!m) return null;
	const n = parseInt(m[1], 16);
	const r = srgbToLinear(((n >> 16) & 255) / 255);
	const g = srgbToLinear(((n >> 8) & 255) / 255);
	const b = srgbToLinear((n & 255) / 255);

	const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
	const m2 = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
	const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);

	return [
		0.2104542553 * l + 0.793617785 * m2 - 0.0040720468 * s,
		1.9779984951 * l - 2.428592205 * m2 + 0.4505937099 * s,
		0.0259040371 * l + 0.7827717662 * m2 - 0.808675766 * s,
	];
}

function oklabToHex(L: number, a: number, b: number): string {
	const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
	const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
	const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;

	const to255 = (v: number) =>
		Math.round(Math.min(1, Math.max(0, linearToSrgb(v))) * 255)
			.toString(16)
			.padStart(2, '0');

	return `#${to255(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s)}${to255(
		-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
	)}${to255(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s)}`;
}

/**
 * Resolve a stored color to something CSS can use.
 *
 *   - a `--cat-*` token key ('orange') → `var(--cat-orange)`, theme-authored
 *   - a custom hex ('#ea580c')         → the same hue, lightness pulled into
 *                                        the current theme's legible band
 *   - anything else                    → null, so the caller can fall back
 *
 * A bare token key must never reach CSS directly: 'orange' is a valid CSS
 * color name and a completely different hue from `--cat-orange`.
 */
export function accentCss(value: string | null | undefined): string | null {
	if (!value) return null;
	if (KEYS.includes(value)) return `var(--cat-${value})`;
	if (!value.startsWith('#')) return null;

	const lab = hexToOklab(value);
	if (!lab) return null;

	const [L, a, b] = lab;
	const band = isDarkTheme() ? BAND.dark : BAND.light;
	const clamped = Math.min(band.max, Math.max(band.min, L));
	// Untouched inside the band — a color that is already legible should come
	// back byte-identical, not "corrected" by a rounding trip.
	if (clamped === L) return value;
	return oklabToHex(clamped, a, b);
}

/**
 * The color for a pinned thing: its chosen one if it has one, otherwise the
 * hash of its url.
 *
 * Keyed on the URL, not on a species. Anything with a route can sit on the
 * Desk — a notebook, an applet, a PDF in Drive, a single day, a person, an
 * external link — so asking "what kind of thing is this?" would be both
 * fragile and beside the point. The url IS the identity.
 */
export function clothFor(pin: { url: string; color?: string | null }): string {
	return accentCss(pin.color) ?? `var(--cat-${pinColor(pin.url)})`;
}
