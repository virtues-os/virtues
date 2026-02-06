/**
 * Theme management utilities for Virtues
 *
 * Single global theme stored in user preferences (database) with localStorage cache.
 * Themes are applied via data-theme attribute on <html> and CSS custom properties.
 */

export type Theme =
	| 'pemberley'
	| 'caladan'
	| 'rivendell'
	| 'oxford'
	| 'lothlorien'
	| 'hogwarts'
	| 'tatooine'
	| 'baker-street'
	| 'narnia'
	| 'canterbury'
	| 'borghese'
	| 'gatsby'
	| 'lyceum'
	| 'asgard'
	| 'agora'
	| 'shire';

const THEME_STORAGE_KEY = 'virtues-theme';
const DEFAULT_THEME: Theme = 'tatooine';

/**
 * Get the current theme from localStorage cache
 */
export function getTheme(): Theme {
	if (typeof window === 'undefined') {
		return DEFAULT_THEME;
	}

	const stored = localStorage.getItem(THEME_STORAGE_KEY) as Theme | null;
	if (stored && isValidTheme(stored)) {
		return stored;
	}

	return DEFAULT_THEME;
}

/**
 * Apply theme to the document (visual only, no persistence)
 */
export function applyTheme(theme: Theme): void {
	if (typeof window === 'undefined') return;

	if (!isValidTheme(theme)) {
		theme = DEFAULT_THEME;
	}

	document.documentElement.setAttribute('data-theme', theme);
	localStorage.setItem(THEME_STORAGE_KEY, theme);
	window.dispatchEvent(new CustomEvent('themechange', { detail: { theme } }));
}

/**
 * Set the theme - applies immediately and persists to database
 */
export async function setTheme(theme: Theme): Promise<void> {
	if (typeof window === 'undefined') return;

	if (!isValidTheme(theme)) {
		console.warn(`Invalid theme: ${theme}. Using default.`);
		theme = DEFAULT_THEME;
	}

	// Apply immediately for instant feedback
	applyTheme(theme);

	// Persist to database
	try {
		const profileRes = await fetch('/api/assistant-profile');
		let existingPrefs = {};
		if (profileRes.ok) {
			const profile = await profileRes.json();
			existingPrefs = profile.ui_preferences || {};
		}

		await fetch('/api/assistant-profile', {
			method: 'PUT',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({
				ui_preferences: {
					...existingPrefs,
					theme
				}
			})
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
	if (typeof window === 'undefined') return DEFAULT_THEME;

	try {
		const response = await fetch('/api/assistant-profile');
		if (response.ok) {
			const profile = await response.json();
			const theme = profile.ui_preferences?.theme as Theme;
			if (theme && isValidTheme(theme)) {
				applyTheme(theme);
				return theme;
			}
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
		'caladan',
		'rivendell',
		'oxford',
		'lothlorien',
		'hogwarts',
		'tatooine',
		'baker-street',
		'narnia',
		'canterbury',
		'borghese',
		'gatsby',
		'lyceum',
		'asgard',
		'agora',
		'shire'
	].includes(theme);
}


/**
 * Get all available themes
 */
export function getAvailableThemes(): Theme[] {
	return [
		'pemberley',
		'caladan',
		'rivendell',
		'oxford',
		'lothlorien',
		'hogwarts',
		'tatooine',
		'baker-street',
		'narnia',
		'canterbury',
		'borghese',
		'gatsby',
		'lyceum',
		'asgard',
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
		lothlorien: 'Lothlorien',
		hogwarts: 'Hogwarts',
		tatooine: 'Tatooine',
		'baker-street': 'Baker Street',
		narnia: 'Narnia',
		canterbury: 'Canterbury',
		borghese: 'Borghese',
		gatsby: 'Gatsby',
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
		surfaceElevated: '#F5F5F5',
		foreground: '#171717',
		foregroundMuted: '#525252',
		primary: '#2883DE',
		syntax: ['#cf222e', '#0a3069', '#8250df', '#0550ae', '#6e7781', '#24292f']
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
		background: '#FFFFFF',
		surface: '#FFFFFF',
		surfaceElevated: '#FAFAFA',
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
		background: '#0C0E13',
		surface: '#161820',
		surfaceElevated: '#1e2028',
		foreground: '#FAF9F5',
		foregroundMuted: '#a8a29e',
		primary: '#FF9D52',
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
	gatsby: {
		background: '#272822',
		surface: '#2d2a2e',
		surfaceElevated: '#3e3d32',
		foreground: '#F8F8F2',
		foregroundMuted: '#908E82',
		primary: '#F92672',
		syntax: ['#f92672', '#e6db74', '#a6e22e', '#ae81ff', '#75715e', '#f8f8f2']
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
		icon: 'ph:building-bold',
		description: 'Austen elegance, pristine'
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
		icon: 'ph:book-open-bold',
		description: 'Academic, studious'
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
		icon: 'ph:lamp-bold',
		description: 'Magical lamppost glow'
	},
	canterbury: {
		icon: 'ph:path-bold',
		description: 'Pilgrim earth tones'
	},
	borghese: {
		icon: 'ph:circle-half-bold',
		description: 'Dramatic light and shadow'
	},
	gatsby: {
		icon: 'ph:champagne-bold',
		description: 'Jazz age vivid glamour'
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
