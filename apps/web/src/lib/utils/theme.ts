/**
 * Theme management utilities for Virtues
 *
 * Single global theme stored in user preferences (database) with localStorage cache.
 * Themes are applied via data-theme attribute on <html> and CSS custom properties.
 */

import { getAssistantProfile, updateAssistantProfile } from '$lib/api/client';

export type Theme =
	| 'pemberley'
	| 'caladan'
	| 'rivendell'
	| 'oxford'
	| 'netherfield'
	| 'lothlorien'
	| 'hogwarts'
	| 'tatooine'
	| 'baker-street'
	| 'narnia'
	| 'canterbury'
	| 'borghese'
	| 'lyceum'
	| 'asgard'
	| 'agora'
	| 'shire';

const THEME_STORAGE_KEY = 'virtues-theme';

/** Resolved `--background` for the active theme; read by app.html pre-paint. */
const THEME_BG_STORAGE_KEY = 'virtues-theme-bg';

/**
 * The two themes Virtues stands behind — one light, one dark.
 *
 * Sixteen themes with no marked pair is sixteen equal strangers: nothing tells
 * you which one the app was designed in, so picking is guesswork and the answer
 * to "just give me dark mode" is a shrug. These two are that answer. They are
 * surfaced by name — Pemberley stays Pemberley — with the role as a qualifier,
 * because the names are part of the product and "Light"/"Dark" are not names.
 *
 * `light` must agree with virtues-registry's DEFAULT_THEME (Rust), which is what
 * a new box actually lands on. Calling a theme the default while new boxes open
 * on a different one would be a label that isn't true.
 */
export const DEFAULT_THEMES = {
	light: 'pemberley',
	dark: 'asgard'
} as const satisfies Record<'light' | 'dark', Theme>;

/** "Virtues Light" / "Virtues Dark" for the two above; null for the rest. */
export function themeDefaultLabel(theme: Theme): string | null {
	if (theme === DEFAULT_THEMES.light) return 'Virtues Light';
	if (theme === DEFAULT_THEMES.dark) return 'Virtues Dark';
	return null;
}

// Fallback theme used only before the API responds (flash prevention).
// The real default is set in virtues-registry (Rust) and delivered via
// /api/assistant-profile; this must match it, or a cold start paints one theme
// and swaps to another the moment the profile lands.
const FALLBACK_THEME: Theme = DEFAULT_THEMES.light;

/**
 * Themes that no longer exist, and where their users go instead.
 *
 * A removed theme can't just fail `isValidTheme` and fall through to the
 * default: someone who chose a dark theme would be dropped onto a white one
 * with no explanation, which reads as the app losing their settings. Gatsby was
 * dark olive with a magenta accent, so Borghese (dark, dramatic) is the nearest
 * surviving neighbour.
 */
const RETIRED_THEMES: Record<string, Theme> = {
	gatsby: 'borghese',
};

/** Resolve a stored theme name, following retirements. */
function resolveTheme(stored: string | null | undefined): Theme | null {
	if (!stored) return null;
	if (isValidTheme(stored)) return stored;
	return RETIRED_THEMES[stored] ?? null;
}

/**
 * Get the current theme from localStorage cache
 */
export function getTheme(): Theme {
	if (typeof window === 'undefined') {
		return FALLBACK_THEME;
	}

	return resolveTheme(localStorage.getItem(THEME_STORAGE_KEY)) ?? FALLBACK_THEME;
}

/**
 * Apply theme to the document (visual only, no persistence)
 */
export function applyTheme(theme: Theme): void {
	if (typeof window === 'undefined') return;

	// Follows retirements, so a stored `gatsby` lands on its successor and is
	// rewritten below rather than silently becoming the default every load.
	theme = resolveTheme(theme) ?? FALLBACK_THEME;

	document.documentElement.setAttribute('data-theme', theme);
	localStorage.setItem(THEME_STORAGE_KEY, theme);

	// Cache the resolved background so the pre-paint script in app.html can
	// paint the right colour on the very first frame — otherwise a cold start
	// (notably the Tauri webview) flashes white before the stylesheet lands,
	// which is worst for anyone on a dark theme. Read back from the cascade
	// rather than duplicating the palette here, so it can't drift.
	const bg = getComputedStyle(document.documentElement)
		.getPropertyValue('--background')
		.trim();
	if (bg) {
		localStorage.setItem(THEME_BG_STORAGE_KEY, bg);
	}

	// Hand the background back to the stylesheet. The bootstrap sets it as an
	// inline style, which outranks any rule — leaving it in place would pin the
	// page to whatever was cached (or to the light fallback on a first run)
	// even after the real theme loaded.
	document.documentElement.style.backgroundColor = '';

	window.dispatchEvent(new CustomEvent('themechange', { detail: { theme } }));
}

/**
 * Set the theme - applies immediately and persists to database
 */
export async function setTheme(theme: Theme): Promise<void> {
	if (typeof window === 'undefined') return;

	if (!isValidTheme(theme)) {
		console.warn(`Invalid theme: ${theme}. Using default.`);
		theme = FALLBACK_THEME;
	}

	// Apply immediately for instant feedback
	applyTheme(theme);

	// Persist to database
	try {
		const profile = await getAssistantProfile<{ ui_preferences?: Record<string, unknown> }>().catch(
			() => null
		);
		const existingPrefs = profile?.ui_preferences || {};

		await updateAssistantProfile({
			ui_preferences: {
				...existingPrefs,
				theme
			}
		});
	} catch (error) {
		console.error('Failed to save theme to database:', error);
	}
}

/**
 * Load theme from database and apply it
 * Call this on app initialization
 */
export async function loadThemeFromDB(): Promise<Theme> {
	if (typeof window === 'undefined') return FALLBACK_THEME;

	try {
		const profile = await getAssistantProfile<{ ui_preferences?: { theme?: string } }>();
		// Through `resolveTheme`, so a retired theme stored in the DB lands on
		// its successor. `setTheme` then writes the successor back, retiring the
		// old name for good rather than remapping it on every load.
		const theme = resolveTheme(profile.ui_preferences?.theme);
		if (theme) {
			if (theme !== profile.ui_preferences?.theme) {
				void setTheme(theme);
			} else {
				applyTheme(theme);
			}
			return theme;
		}
	} catch (error) {
		console.error('Failed to load theme from database:', error);
	}

	// Fall back to localStorage or default
	const cached = getTheme();
	applyTheme(cached);
	return cached;
}

/**
 * Initialize theme on page load
 * Uses localStorage cache for instant display, then syncs with DB
 */
export function initTheme(): void {
	if (typeof window === 'undefined') return;

	// Apply cached theme immediately (no flash)
	const cached = getTheme();
	document.documentElement.setAttribute('data-theme', cached);

	// Then load from DB and update if different
	loadThemeFromDB();
}

/**
 * Type guard to check if a string is a valid theme
 */
export function isValidTheme(theme: string): theme is Theme {
	return [
		'pemberley',
		'oxford',
		'caladan',
		'rivendell',
		'netherfield',
		'lothlorien',
		'hogwarts',
		'tatooine',
		'baker-street',
		'narnia',
		'canterbury',
		'borghese',
		'lyceum',
		'asgard',
		'agora',
		'shire'
	].includes(theme);
}


/**
 * Whether a theme reads as dark (its background luminance is below mid-gray).
 * Used by the native mobile shell to pick status-bar / keyboard appearance —
 * themes are user-picked, so darkness can't be inferred from the OS setting.
 */
export function isThemeDark(theme: Theme): boolean {
	const hex = themePreviewColors[theme]?.background ?? '#ffffff';
	const n = parseInt(hex.slice(1), 16);
	const r = (n >> 16) & 0xff;
	const g = (n >> 8) & 0xff;
	const b = n & 0xff;
	// Perceived luminance (ITU-R BT.601), 0–255.
	return 0.299 * r + 0.587 * g + 0.114 * b < 128;
}

/**
 * Get all available themes.
 *
 * The two defaults lead, light then dark, so the pair reads as a pair. Every
 * surface that lists themes uses this order, which is the only thing keeping
 * "recommended" from meaning something different in ⌘K than in Settings.
 */
export function getAvailableThemes(): Theme[] {
	return [
		DEFAULT_THEMES.light,
		DEFAULT_THEMES.dark,
		'oxford',
		'caladan',
		'rivendell',
		'netherfield',
		'lothlorien',
		'hogwarts',
		'tatooine',
		'baker-street',
		'narnia',
		'canterbury',
		'borghese',
		'lyceum',
		'agora',
		'shire'
	];
}

/**
 * Get theme display name
 */
export function getThemeDisplayName(theme: Theme): string {
	const names: Record<Theme, string> = {
		pemberley: 'Pemberley',
		caladan: 'Caladan',
		rivendell: 'Rivendell',
		oxford: 'Oxford',
		netherfield: 'Netherfield',
		lothlorien: 'Lothlorien',
		hogwarts: 'Hogwarts',
		tatooine: 'Tatooine',
		'baker-street': 'Baker Street',
		narnia: 'Narnia',
		canterbury: 'Canterbury',
		borghese: 'Borghese',
		lyceum: 'The Lyceum',
		asgard: 'Asgard',
		agora: 'Agora',
		shire: 'The Shire'
	};
	return names[theme];
}

/**
 * Theme preview colors for theme cards
 */
export const themePreviewColors: Record<
	Theme,
	{
		background: string;
		surface: string;
		surfaceElevated: string;
		foreground: string;
		foregroundMuted: string;
		primary: string;
		// Syntax highlighting colors for code preview
		syntax: string[];
	}
> = {
	pemberley: {
		background: '#FFFFFF',
		surface: '#FFFFFF',
		surfaceElevated: '#FFFFFF',
		foreground: '#17171A',
		foregroundMuted: '#3F3F46',
		primary: '#0A84FF',
		syntax: ['#C4322B', '#0A84FF', '#7C3AED', '#0060DF', '#71717A', '#17171A']
	},
	caladan: {
		background: '#FFFFFF',
		surface: '#FFFFFF',
		surfaceElevated: '#F5F5F5',
		foreground: '#171717',
		foregroundMuted: '#525252',
		primary: '#2883DE',
		syntax: ['#cf222e', '#0a3069', '#8250df', '#0550ae', '#6e7781', '#24292f']
	},
	rivendell: {
		background: '#FFFFFF',
		surface: '#FFFFFF',
		surfaceElevated: '#F5F5F5',
		foreground: '#171717',
		foregroundMuted: '#525252',
		primary: '#D97757',
		syntax: ['#cf222e', '#0a3069', '#8250df', '#0550ae', '#6e7781', '#24292f']
	},
	oxford: {
		background: '#FDFCF9',
		surface: '#FFFFFF',
		surfaceElevated: '#F4F3F0',
		foreground: '#1A2030',
		foregroundMuted: '#3E4459',
		primary: '#1E3159',
		syntax: ['#9A2B2E', '#1E3159', '#7E2225', '#1E4E8C', '#6C7185', '#1A2030']
	},
	netherfield: {
		background: '#FFFFFF',
		surface: '#FFFFFF',
		surfaceElevated: '#F5F5F5',
		foreground: '#171717',
		foregroundMuted: '#525252',
		primary: '#2883DE',
		syntax: ['#cf222e', '#0a3069', '#8250df', '#0550ae', '#6e7781', '#24292f']
	},
	lothlorien: {
		background: '#1a1a2e',
		surface: '#1f1f35',
		surfaceElevated: '#25253d',
		foreground: '#e8e8f0',
		foregroundMuted: '#a0a0b8',
		primary: '#E8A87C',
		syntax: ['#ff7b72', '#a5d6ff', '#d2a8ff', '#79c0ff', '#8b949e', '#e6edf3']
	},
	hogwarts: {
		background: '#F7F7F4',
		surface: '#FFFFFF',
		surfaceElevated: '#F0EFE9',
		foreground: '#26251E',
		foregroundMuted: '#3D3B33',
		primary: '#EB5601',
		syntax: ['#cf222e', '#0a3069', '#8250df', '#0550ae', '#6e7781', '#24292f']
	},
	tatooine: {
		background: '#fdf6e3',
		surface: '#fdf6e3',
		surfaceElevated: '#eee8d5',
		foreground: '#2d3632',
		foregroundMuted: '#5d665e',
		primary: '#268bd2',
		syntax: ['#859900', '#2aa198', '#268bd2', '#cb4b16', '#8f918a', '#5d665e']
	},
	'baker-street': {
		background: '#0a0a0a',
		surface: '#171717',
		surfaceElevated: '#262626',
		foreground: '#fafafa',
		foregroundMuted: '#a3a3a3',
		primary: '#60a5fa',
		syntax: ['#ff7b72', '#a5d6ff', '#d2a8ff', '#79c0ff', '#8b949e', '#e6edf3']
	},
	narnia: {
		background: '#0F1821',
		surface: '#131E28',
		surfaceElevated: '#18242F',
		foreground: '#EDF1F4',
		foregroundMuted: '#9BA7B2',
		primary: '#7CC3DE',
		syntax: ['#bb9af7', '#9ece6a', '#7aa2f7', '#ff9e64', '#565f89', '#a9b1d6']
	},
	canterbury: {
		background: '#14120B',
		surface: '#1B1913',
		surfaceElevated: '#221E15',
		foreground: '#EDECEC',
		foregroundMuted: '#A9A39A',
		primary: '#E4B873',
		syntax: ['#cb7676', '#c98a7d', '#80a665', '#e6cc77', '#758575', '#dbd7ca']
	},
	borghese: {
		background: '#000000',
		surface: '#000000',
		surfaceElevated: '#1a1a1a',
		foreground: '#FFFFFF',
		foregroundMuted: '#999999',
		primary: '#FFFFFF',
		syntax: ['#ff9492', '#addcff', '#dcbdfb', '#91cbff', '#9198a1', '#f0f3f6']
	},
	lyceum: {
		background: '#292d34',
		surface: '#2f333d',
		surfaceElevated: '#383e4a',
		foreground: '#c8cdd6',
		foregroundMuted: '#7c8490',
		primary: '#61afef',
		syntax: ['#c678dd', '#98c379', '#61afef', '#d19a66', '#5c6370', '#abb2bf']
	},
	asgard: {
		background: '#141414',
		surface: '#181818',
		surfaceElevated: '#1e1e1e',
		foreground: '#D4D4D4',
		foregroundMuted: '#898989',
		primary: '#88C0D0',
		syntax: ['#cb7676', '#c98a7d', '#80a665', '#e6cc77', '#758575', '#dbd7ca']
	},
	agora: {
		background: '#282a36',
		surface: '#2d303e',
		surfaceElevated: '#343746',
		foreground: '#f8f8f2',
		foregroundMuted: '#6272a4',
		primary: '#ff79c6',
		syntax: ['#ff79c6', '#f1fa8c', '#50fa7b', '#bd93f9', '#6272a4', '#f8f8f2']
	},
	shire: {
		background: '#232136',
		surface: '#2a273f',
		surfaceElevated: '#312e47',
		foreground: '#e0def4',
		foregroundMuted: '#908caa',
		primary: '#ea9a97',
		syntax: ['#c4a7e7', '#f6c177', '#9ccfd8', '#ea9a97', '#6e6a86', '#e0def4']
	}
};

/**
 * Theme metadata for theme selection UI
 */
export const themeMetadata: Record<
	Theme,
	{
		icon: string;
		description: string;
	}
> = {
	pemberley: {
		icon: 'ph:circle-bold',
		description: 'White on white, hairlines & one blue'
	},
	caladan: {
		icon: 'ph:waves-bold',
		description: 'Atreides ocean world'
	},
	rivendell: {
		icon: 'ph:leaf-bold',
		description: 'Elven refuge, warm light'
	},
	oxford: {
		icon: 'ph:feather-bold',
		description: 'Heritage, navy & claret'
	},
	netherfield: {
		icon: 'ph:building-bold',
		description: 'Austen elegance, pristine'
	},
	lothlorien: {
		icon: 'ph:tree-bold',
		description: 'Golden wood twilight'
	},
	hogwarts: {
		icon: 'ph:magic-wand-bold',
		description: 'Warm parchment, candlelit'
	},
	tatooine: {
		icon: 'ph:sun-bold',
		description: 'Twin suns, desert warmth'
	},
	'baker-street': {
		icon: 'ph:magnifying-glass-bold',
		description: 'Victorian gaslight'
	},
	narnia: {
		icon: 'ph:compass-tool-bold',
		description: 'Blueprint under lamplight'
	},
	canterbury: {
		icon: 'ph:path-bold',
		description: 'Pilgrim earth tones'
	},
	borghese: {
		icon: 'ph:circle-half-bold',
		description: 'Dramatic light and shadow'
	},
	lyceum: {
		icon: 'ph:student-bold',
		description: 'Aristotelian cool blues'
	},
	asgard: {
		icon: 'ph:lightning-bold',
		description: 'Norse realm, cold majesty'
	},
	agora: {
		icon: 'ph:columns-bold',
		description: 'Greek marketplace purple'
	},
	shire: {
		icon: 'ph:house-bold',
		description: 'Cozy hobbit pastels'
	}
};
