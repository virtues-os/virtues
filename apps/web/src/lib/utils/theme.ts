/**
 * Theme management utilities for Virtues
 *
 * Provides functions to get, set, and toggle themes across the application.
 * Themes are stored in localStorage and applied via data-theme attribute on <html>.
 */

export type Theme = 'warm' | 'light' | 'dark' | 'night';

const THEME_STORAGE_KEY = 'virtues-theme';
const DEFAULT_THEME: Theme = 'light';

/**
 * Get the current theme from localStorage or system preference
 */
export function getTheme(): Theme {
	if (typeof window === 'undefined') {
		return DEFAULT_THEME;
	}

	// Check localStorage first
	const stored = localStorage.getItem(THEME_STORAGE_KEY) as Theme | null;
	if (stored && isValidTheme(stored)) {
		return stored;
	}

	return DEFAULT_THEME;
}

/**
 * Set the theme and persist to localStorage
 */
export function setTheme(theme: Theme): void {
	if (typeof window === 'undefined') {
		return;
	}

	// Validate theme
	if (!isValidTheme(theme)) {
		console.warn(`Invalid theme: ${theme}. Using default.`);
		theme = DEFAULT_THEME;
	}

	// Apply to document
	document.documentElement.setAttribute('data-theme', theme);

	// Persist to localStorage
	localStorage.setItem(THEME_STORAGE_KEY, theme);

	// Dispatch custom event for listeners
	window.dispatchEvent(new CustomEvent('themechange', { detail: { theme } }));
}

/**
 * Toggle between light and dark themes
 */
export function toggleTheme(): void {
	const current = getTheme();
	const next = current === 'dark' ? 'light' : 'dark';
	setTheme(next);
}

/**
 * Initialize theme on page load
 * Call this in your root layout or app component
 */
export function initTheme(): void {
	if (typeof window === 'undefined') {
		return;
	}

	const theme = getTheme();
	document.documentElement.setAttribute('data-theme', theme);
}


/**
 * Type guard to check if a string is a valid theme
 */
function isValidTheme(theme: string): theme is Theme {
	return ['warm', 'light', 'dark', 'night'].includes(theme);
}

/**
 * Get all available themes
 */
export function getAvailableThemes(): Theme[] {
	return ['warm', 'light', 'dark', 'night'];
}

/**
 * Get theme display name
 */
export function getThemeDisplayName(theme: Theme): string {
	const names: Record<Theme, string> = {
		warm: 'Warm',
		light: 'Light (default)',
		dark: 'Dark',
		night: 'Night'
	};
	return names[theme];
}

/**
 * Theme preview colors for VSCode-style theme cards
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
	warm: {
		background: '#FAF9F5',
		surface: '#FFFFFF',
		surfaceElevated: '#F3F2E9',
		foreground: '#14283D',
		foregroundMuted: '#1a3550',
		primary: '#2883DE',
		syntax: ['#7C3AED', '#DC2626', '#059669', '#D97706', '#2563EB', '#DB2777']
	},
	light: {
		background: '#FFFFFF',
		surface: '#FAFAFA',
		surfaceElevated: '#F5F5F5',
		foreground: '#171717',
		foregroundMuted: '#525252',
		primary: '#2563EB',
		syntax: ['#6366F1', '#7C3AED', '#EC4899', '#EF4444', '#F59E0B', '#10B981']
	},
	dark: {
		background: '#0a0a0a',
		surface: '#171717',
		surfaceElevated: '#262626',
		foreground: '#fafafa',
		foregroundMuted: '#a3a3a3',
		primary: '#60a5fa',
		syntax: ['#C084FC', '#F472B6', '#FBBF24', '#F87171', '#34D399', '#60A5FA']
	},
	night: {
		background: '#0C0E13',
		surface: '#1a1d24',
		surfaceElevated: '#252830',
		foreground: '#E8E6E3',
		foregroundMuted: '#9CA3AF',
		primary: '#FF9141',
		syntax: ['#7DD3FC', '#A78BFA', '#F472B6', '#FB923C', '#4ADE80', '#FBBF24']
	}
};

/**
 * Theme metadata for onboarding cards
 */
export const themeMetadata: Record<
	Theme,
	{
		icon: string;
		description: string;
	}
> = {
	warm: {
		icon: 'ph:sun-horizon-bold',
		description: 'Warm sepia tones, easy on the eyes'
	},
	light: {
		icon: 'ph:sun-bold',
		description: 'Clean and bright, paper-like'
	},
	dark: {
		icon: 'ph:moon-bold',
		description: 'True dark mode for low light'
	},
	night: {
		icon: 'ph:moon-stars-bold',
		description: 'Dark with warm orange accents'
	}
};
