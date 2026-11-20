/**
 * Theme management for Virtues (agents/build/design.md, "Themes").
 *
 * Two themes paint the app: Oxford (Light) and Asgard (Dark). What a person
 * picks is a choice: Light, Dark, Match system, which follows the device's
 * `prefers-color-scheme`, or Follow the day, which is Light from sunrise to
 * sunset and Dark otherwise. The choice is stored in the assistant profile's
 * `ui_preferences.theme` and cached in localStorage; the theme it resolves to
 * is set as `data-theme` on <html>.
 */

import { getAssistantProfile, request, updateUiPreferences } from '$lib/api/client';

/** A theme `themes.css` defines. */
export type Theme = 'oxford' | 'asgard';

/** What a person picks. `system` and `day` resolve to one of the two themes. */
export type ThemeChoice = Theme | 'system' | 'day';

const THEME_STORAGE_KEY = 'virtues-theme';

/**
 * Today's sunrise and sunset for Follow the day, as "<minutes> <minutes>"
 * after local midnight. app.html's pre-paint reads it too, so the first
 * frame paints the right theme.
 */
const SUN_STORAGE_KEY = 'virtues-sun';

/** Sunrise and sunset when the server has no forecast yet: 07:00 and 19:00. */
const DEFAULT_SUN: readonly [number, number] = [7 * 60, 19 * 60];

/** Resolved `--background` for the active theme; read by app.html pre-paint. */
const THEME_BG_STORAGE_KEY = 'virtues-theme-bg';

/**
 * Resolved `--page`, which is what a COLD START actually ends up looking at.
 *
 * Both shells cover the window in the page: the phone's `.viewport` and the
 * desktop's main card both take `var(--color-page)`. Painting anything else
 * first shows one colour for a few hundred milliseconds and then steps to
 * another under it. The key keeps its name because app.html and the Mac
 * app's connect.html read it.
 */
const THEME_SURFACE_STORAGE_KEY = 'virtues-theme-surface';

/**
 * The two themes, by role.
 *
 * `light` must agree with virtues-registry's DEFAULT_THEME (Rust), which is
 * what a new box lands on, or a cold start paints one theme and swaps to
 * another the moment the profile arrives.
 */
export const DEFAULT_THEMES = {
	light: 'oxford',
	dark: 'asgard'
} as const satisfies Record<'light' | 'dark', Theme>;

const FALLBACK_CHOICE: ThemeChoice = DEFAULT_THEMES.light;

/**
 * Theme names that no longer exist, and where their users go.
 *
 * A removed name can't just fail validation and fall through to the default:
 * someone who chose a dark theme would be dropped onto a white page, which
 * reads as the app losing their settings. Each lands on the theme that
 * matches its darkness.
 */
const RETIRED_THEMES: Record<string, Theme> = {
	pemberley: 'oxford',
	caladan: 'oxford',
	rivendell: 'oxford',
	netherfield: 'oxford',
	hogwarts: 'oxford',
	tatooine: 'oxford',
	lothlorien: 'asgard',
	'baker-street': 'asgard',
	narnia: 'asgard',
	canterbury: 'asgard',
	borghese: 'asgard',
	lyceum: 'asgard',
	agora: 'asgard',
	shire: 'asgard',
	gatsby: 'asgard'
};

/** Whether a string is a choice a person can make. */
export function isValidTheme(value: string): value is ThemeChoice {
	return value === 'oxford' || value === 'asgard' || value === 'system' || value === 'day';
}

/** Resolve a stored name, following retirements. */
function resolveChoice(stored: string | null | undefined): ThemeChoice | null {
	if (!stored) return null;
	if (isValidTheme(stored)) return stored;
	return RETIRED_THEMES[stored] ?? null;
}

const darkQuery = (): MediaQueryList | null =>
	typeof window === 'undefined' || !window.matchMedia ? null : window.matchMedia('(prefers-color-scheme: dark)');

/** Cached sunrise and sunset, in minutes after local midnight. */
function sunTimes(): readonly [number, number] {
	try {
		const [rise, set] = (localStorage.getItem(SUN_STORAGE_KEY) ?? '').split(' ').map(Number);
		if (Number.isFinite(rise) && Number.isFinite(set) && rise < set) return [rise, set];
	} catch {
		// Storage disabled: the default hours.
	}
	return DEFAULT_SUN;
}

const minutesOf = (d: Date) => d.getHours() * 60 + d.getMinutes();

/** Whether it is between today's sunrise and sunset. */
export function isDaytime(now: Date = new Date()): boolean {
	const [rise, set] = sunTimes();
	const m = minutesOf(now);
	return m >= rise && m < set;
}

/** The theme a choice paints right now. */
export function themeFor(choice: ThemeChoice): Theme {
	if (choice === 'day') return isDaytime() ? DEFAULT_THEMES.light : DEFAULT_THEMES.dark;
	if (choice !== 'system') return choice;
	return darkQuery()?.matches ? DEFAULT_THEMES.dark : DEFAULT_THEMES.light;
}

/** The stored choice, from the localStorage cache. */
export function getThemeChoice(): ThemeChoice {
	if (typeof window === 'undefined') return FALLBACK_CHOICE;
	try {
		return resolveChoice(localStorage.getItem(THEME_STORAGE_KEY)) ?? FALLBACK_CHOICE;
	} catch {
		return FALLBACK_CHOICE;
	}
}

/** The theme painted now: the stored choice, resolved. */
export function getTheme(): Theme {
	return themeFor(getThemeChoice());
}

/** Re-resolves Match system when the device switches between light and dark. */
let systemListener: (() => void) | null = null;

function followSystem(on: boolean): void {
	const query = darkQuery();
	if (!query) return;
	if (systemListener) {
		query.removeEventListener('change', systemListener);
		systemListener = null;
	}
	if (on) {
		systemListener = () => paint(themeFor('system'), 'system');
		query.addEventListener('change', systemListener);
	}
}

/**
 * Follow the day: repaint at the next sunrise or sunset, and when the window
 * comes back (a sleeping laptop misses its timer). Sun times come from the
 * server's weather forecast for today, which is made for its last known
 * location; without one the day runs from 07:00 to 19:00.
 */
let dayTimer: ReturnType<typeof setTimeout> | null = null;
let dayVisibility: (() => void) | null = null;

function followDay(on: boolean): void {
	if (dayTimer) clearTimeout(dayTimer);
	dayTimer = null;
	if (dayVisibility) document.removeEventListener('visibilitychange', dayVisibility);
	dayVisibility = null;
	if (!on) return;

	const repaint = () => {
		if (getThemeChoice() === 'day') paint(themeFor('day'), 'day');
	};
	const schedule = () => {
		if (dayTimer) clearTimeout(dayTimer);
		const [rise, set] = sunTimes();
		const now = new Date();
		const m = minutesOf(now);
		const next = m < rise ? rise : m < set ? set : rise + 24 * 60;
		const at = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 0, next, 5);
		dayTimer = setTimeout(() => {
			repaint();
			void refreshSun().then(repaint);
			schedule();
		}, at.getTime() - now.getTime());
	};
	dayVisibility = () => {
		if (document.visibilityState !== 'visible') return;
		repaint();
		schedule();
	};
	document.addEventListener('visibilitychange', dayVisibility);
	schedule();
	void refreshSun().then(() => {
		repaint();
		schedule();
	});
}

/** Today's sunrise and sunset from the server's forecast, cached for the pre-paint. */
async function refreshSun(): Promise<void> {
	try {
		const weather = await request<{ sunrise?: string | null; sunset?: string | null } | null>('/weather/current');
		if (!weather?.sunrise || !weather.sunset) return;
		const rise = minutesOf(new Date(weather.sunrise));
		const set = minutesOf(new Date(weather.sunset));
		if (rise < set) localStorage.setItem(SUN_STORAGE_KEY, `${rise} ${set}`);
	} catch {
		// No forecast or no server: keep the cached or default hours.
	}
}

function paint(theme: Theme, choice: ThemeChoice): void {
	document.documentElement.setAttribute('data-theme', theme);

	// Cache the resolved colours so the pre-paint script in app.html paints
	// the right one on the very first frame. Read back from the cascade rather
	// than duplicating the palette here, so it can't drift.
	const styles = getComputedStyle(document.documentElement);
	try {
		const bg = styles.getPropertyValue('--background').trim();
		if (bg) localStorage.setItem(THEME_BG_STORAGE_KEY, bg);
		const page = styles.getPropertyValue('--page').trim();
		if (page) localStorage.setItem(THEME_SURFACE_STORAGE_KEY, page);
	} catch {
		// Storage disabled: the stylesheet still paints.
	}

	// Hand the background back to the stylesheet. The bootstrap sets it as an
	// inline style, which outranks any rule.
	document.documentElement.style.backgroundColor = '';

	window.dispatchEvent(new CustomEvent('themechange', { detail: { theme, choice } }));
}

/** Apply a choice to the document (visual only, no persistence). */
export function applyTheme(choice: ThemeChoice): void {
	if (typeof window === 'undefined') return;
	choice = resolveChoice(choice) ?? FALLBACK_CHOICE;
	try {
		localStorage.setItem(THEME_STORAGE_KEY, choice);
	} catch {
		// Storage disabled: the choice lasts for this page.
	}
	followSystem(choice === 'system');
	followDay(choice === 'day');
	paint(themeFor(choice), choice);
}

/** Apply a choice and save it to the profile. */
export async function setTheme(choice: ThemeChoice): Promise<void> {
	if (typeof window === 'undefined') return;
	if (!isValidTheme(choice)) choice = FALLBACK_CHOICE;
	applyTheme(choice);
	try {
		await updateUiPreferences({ theme: choice });
	} catch (error) {
		console.error('Failed to save theme to database:', error);
	}
}

/**
 * Load the choice from the profile and apply it. A retired name is written
 * back as its successor, so it is remapped once rather than on every load.
 */
export async function loadThemeFromDB(): Promise<ThemeChoice> {
	if (typeof window === 'undefined') return FALLBACK_CHOICE;

	try {
		const profile = await getAssistantProfile<{ ui_preferences?: { theme?: string; accent?: string } }>();
		applyAccent(isAccent(profile.ui_preferences?.accent) ? profile.ui_preferences.accent : DEFAULT_ACCENT);
		const stored = profile.ui_preferences?.theme;
		const choice = resolveChoice(stored);
		if (choice) {
			if (choice !== stored) void setTheme(choice);
			else applyTheme(choice);
			return choice;
		}
	} catch (error) {
		console.error('Failed to load theme from database:', error);
	}

	const cached = getThemeChoice();
	applyTheme(cached);
	return cached;
}

/** Apply the cached choice at once, then sync with the profile. */
export function initTheme(): void {
	if (typeof window === 'undefined') return;
	applyAccent(getAccent());
	applyTheme(getThemeChoice());
	void loadThemeFromDB();
}

/**
 * Whether a theme is dark. The native mobile shell uses it to pick the status
 * bar and keyboard appearance.
 */
export function isThemeDark(theme: Theme): boolean {
	return theme === DEFAULT_THEMES.dark;
}

/** Every choice, in the order every list shows them. */
export function getAvailableThemes(): ThemeChoice[] {
	return [DEFAULT_THEMES.light, DEFAULT_THEMES.dark, 'system', 'day'];
}

const DISPLAY_NAMES: Record<ThemeChoice, string> = {
	oxford: 'Light',
	asgard: 'Dark',
	system: 'Match system',
	day: 'Follow the day'
};

/** A choice's name in the interface. */
export function getThemeDisplayName(choice: ThemeChoice): string {
	return DISPLAY_NAMES[choice];
}

/** Preview colors for the theme cards. The accent is the chosen swatch's
 * tuning for each theme, so the cards follow the Accent setting. */
export const themePreviewColors: Record<
	Theme,
	{
		background: string;
		surface: string;
		surfaceElevated: string;
		foreground: string;
		foregroundMuted: string;
		primary: string;
	}
> = {
	oxford: {
		background: '#FCFCFC',
		surface: '#FFFFFF',
		surfaceElevated: '#F3F3F3',
		foreground: '#1A2030',
		foregroundMuted: '#3E4459',
		primary: 'var(--accent-light)'
	},
	asgard: {
		background: '#0E0E0F',
		surface: '#141416',
		surfaceElevated: '#1B1C1F',
		foreground: '#EDEDEF',
		foregroundMuted: '#A1A2A8',
		primary: 'var(--accent-dark)'
	}
};

/** Icon and one line for each choice in the ⌘K theme list. */
export const themeMetadata: Record<ThemeChoice, { icon: string; description: string }> = {
	oxford: { icon: 'mynaui:sun', description: 'Dark ink on white' },
	asgard: { icon: 'mynaui:moon', description: 'Light ink on near-black' },
	system: { icon: 'mynaui:circle-half', description: 'Follows your device' },
	day: { icon: 'mynaui:sunrise', description: 'Light from sunrise to sunset' }
};

// ── Accent ──────────────────────────────────────────────────────────────────
// The one color a page carries (design.md, "Accent"): a swatch that
// themes.css tunes once for Light and once for Dark, or a color the person
// chooses, which tuneAccent fits to both themes here. It is saved on the
// profile beside the theme (`ui_preferences.accent`, a swatch id or a
// `#rrggbb`), cached for app.html's pre-paint, and set as `data-accent` on
// <html>. A chosen color sets `data-accent="custom"` and its two tunings as
// --accent-light and --accent-dark on <html>.

export type AccentSwatch = 'navy' | 'blue' | 'indigo' | 'purple' | 'pink' | 'teal' | 'graphite';

/** A swatch id, or a color the person chose as `#rrggbb`. */
export type Accent = AccentSwatch | `#${string}`;

export const DEFAULT_ACCENT: Accent = 'navy';

/** Every swatch, in the order the picker shows them. */
export const ACCENTS: readonly { id: AccentSwatch; name: string }[] = [
	{ id: 'navy', name: 'Navy' },
	{ id: 'blue', name: 'Blue' },
	{ id: 'indigo', name: 'Indigo' },
	{ id: 'purple', name: 'Purple' },
	{ id: 'pink', name: 'Pink' },
	{ id: 'teal', name: 'Teal' },
	{ id: 'graphite', name: 'Graphite' }
];

const ACCENT_STORAGE_KEY = 'virtues-accent';
/** A chosen color's two tunings, "light dark", for app.html's pre-paint. */
const ACCENT_TUNED_STORAGE_KEY = 'virtues-accent-tuned';

const HEX = /^#[0-9a-f]{6}$/i;

/** Whether a value is a chosen color rather than a swatch. */
export function isCustomAccent(value: unknown): value is `#${string}` {
	return typeof value === 'string' && HEX.test(value);
}

export function isAccent(value: unknown): value is Accent {
	return isCustomAccent(value) || (typeof value === 'string' && ACCENTS.some((a) => a.id === value));
}

/** The cached accent. */
export function getAccent(): Accent {
	if (typeof window === 'undefined') return DEFAULT_ACCENT;
	try {
		const stored = localStorage.getItem(ACCENT_STORAGE_KEY);
		return isAccent(stored) ? stored : DEFAULT_ACCENT;
	} catch {
		return DEFAULT_ACCENT;
	}
}

// Contrast, as WCAG measures it.
function luminance(hex: string): number {
	const [r, g, b] = [1, 3, 5].map((i) => {
		const c = parseInt(hex.slice(i, i + 2), 16) / 255;
		return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
	});
	return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
}

function toHsl(hex: string): [number, number, number] {
	const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
	const max = Math.max(r, g, b);
	const min = Math.min(r, g, b);
	const l = (max + min) / 2;
	if (max === min) return [0, 0, l];
	const d = max - min;
	const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
	const h = max === r ? (g - b) / d + (g < b ? 6 : 0) : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
	return [h * 60, s, l];
}

function fromHsl(h: number, s: number, l: number): string {
	const k = (n: number) => (n + h / 30) % 12;
	const a = s * Math.min(l, 1 - l);
	const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1));
	return '#' + [0, 8, 4].map((n) => Math.round(f(n) * 255).toString(16).padStart(2, '0')).join('').toUpperCase();
}

/**
 * Walk a color's lightness toward `step`'s direction until it clears
 * `target` against `page`, keeping its hue and saturation.
 */
function fit(hex: string, page: string, target: number, step: number): string {
	const [h, s, l0] = toHsl(hex);
	let out = hex.toUpperCase();
	for (let l = l0; contrast(out, page) < target && l >= 0 && l <= 1; l += step) {
		out = fromHsl(h, s, l);
	}
	return out;
}

/**
 * A chosen color, fitted to both themes the way the swatches are tuned by
 * hand: darkened until links and the filled button's white label clear
 * 4.6:1 on Light's page, and lightened until they clear 7:1 on Dark's,
 * where the swatches sit. A color that already clears is kept as it is.
 */
export function tuneAccent(hex: string): { light: string; dark: string } {
	return {
		light: fit(hex, themePreviewColors.oxford.surface, 4.6, -0.005),
		dark: fit(hex, themePreviewColors.asgard.surface, 7, 0.005)
	};
}

/** Paint an accent (visual only, no persistence). */
export function applyAccent(accent: Accent): void {
	if (typeof window === 'undefined') return;
	const root = document.documentElement;
	let tuned: string | null = null;
	if (isCustomAccent(accent)) {
		const { light, dark } = tuneAccent(accent);
		root.style.setProperty('--accent-light', light);
		root.style.setProperty('--accent-dark', dark);
		root.setAttribute('data-accent', 'custom');
		tuned = `${light} ${dark}`;
	} else {
		root.style.removeProperty('--accent-light');
		root.style.removeProperty('--accent-dark');
		root.setAttribute('data-accent', accent);
	}
	try {
		localStorage.setItem(ACCENT_STORAGE_KEY, accent);
		if (tuned) localStorage.setItem(ACCENT_TUNED_STORAGE_KEY, tuned);
		else localStorage.removeItem(ACCENT_TUNED_STORAGE_KEY);
	} catch {
		// Storage disabled: the accent lasts for this page.
	}
	window.dispatchEvent(new CustomEvent('accentchange', { detail: { accent } }));
}

/** Paint an accent and save it to the profile. */
export async function setAccent(accent: Accent): Promise<void> {
	if (typeof window === 'undefined') return;
	applyAccent(accent);
	try {
		await updateUiPreferences({ accent });
	} catch (error) {
		console.error('Failed to save accent:', error);
	}
}
