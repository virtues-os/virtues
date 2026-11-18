/**
 * Theme management for Virtues (agents/build/design.md, "Themes").
 *
 * Two themes paint the app: Oxford (Light) and Asgard (Dark). What a person
 * picks is a choice: Light, Dark, or Match system, which follows the
 * device's `prefers-color-scheme`. The choice is stored in the assistant
 * profile's `ui_preferences.theme` and cached in localStorage; the theme it
 * resolves to is set as `data-theme` on <html>.
 */

import { getAssistantProfile, updateUiPreferences } from '$lib/api/client';

/** A theme `themes.css` defines. */
export type Theme = 'oxford' | 'asgard';

/** What a person picks. `system` resolves to one of the two themes. */
export type ThemeChoice = Theme | 'system';

const THEME_STORAGE_KEY = 'virtues-theme';

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
	return value === 'oxford' || value === 'asgard' || value === 'system';
}

/** Resolve a stored name, following retirements. */
function resolveChoice(stored: string | null | undefined): ThemeChoice | null {
	if (!stored) return null;
	if (isValidTheme(stored)) return stored;
	return RETIRED_THEMES[stored] ?? null;
}

const darkQuery = (): MediaQueryList | null =>
	typeof window === 'undefined' || !window.matchMedia ? null : window.matchMedia('(prefers-color-scheme: dark)');

/** The theme a choice paints right now. */
export function themeFor(choice: ThemeChoice): Theme {
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
	return [DEFAULT_THEMES.light, DEFAULT_THEMES.dark, 'system'];
}

const DISPLAY_NAMES: Record<ThemeChoice, string> = {
	oxford: 'Light',
	asgard: 'Dark',
	system: 'Match system'
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
		background: '#FDFCF9',
		surface: '#FFFFFF',
		surfaceElevated: '#F4F3F0',
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
	system: { icon: 'mynaui:circle-half', description: 'Follows your device' }
};

// ── Accent ──────────────────────────────────────────────────────────────────
// The one color a page carries (design.md, "Accent"), picked from swatches
// that themes.css tunes once for Light and once for Dark. It is saved on the
// profile beside the theme (`ui_preferences.accent`), cached for app.html's
// pre-paint, and set as `data-accent` on <html>.

export type Accent = 'navy' | 'blue' | 'indigo' | 'purple' | 'pink' | 'teal' | 'graphite';

export const DEFAULT_ACCENT: Accent = 'navy';

/** Every swatch, in the order the picker shows them. */
export const ACCENTS: readonly { id: Accent; name: string }[] = [
	{ id: 'navy', name: 'Navy' },
	{ id: 'blue', name: 'Blue' },
	{ id: 'indigo', name: 'Indigo' },
	{ id: 'purple', name: 'Purple' },
	{ id: 'pink', name: 'Pink' },
	{ id: 'teal', name: 'Teal' },
	{ id: 'graphite', name: 'Graphite' }
];

const ACCENT_STORAGE_KEY = 'virtues-accent';

export function isAccent(value: unknown): value is Accent {
	return typeof value === 'string' && ACCENTS.some((a) => a.id === value);
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

/** Paint an accent (visual only, no persistence). */
export function applyAccent(accent: Accent): void {
	if (typeof window === 'undefined') return;
	document.documentElement.setAttribute('data-accent', accent);
	try {
		localStorage.setItem(ACCENT_STORAGE_KEY, accent);
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
