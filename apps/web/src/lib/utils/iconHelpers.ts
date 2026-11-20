/**
 * Check if a value is an emoji (vs a Remix Icon name like "mynaui:folder").
 * Emojis are raw Unicode characters and don't contain ":".
 */
export function isEmoji(val: string): boolean {
	return !val.includes(':');
}

/**
 * A thread's glyph when it has none of its own: the Threads door's glyph.
 * Every surface that shows a thread reads this, so the tab, the row, the
 * menu and the trash agree.
 */
export const THREAD_ICON = 'atlas:projects';

/**
 * The Atlas glyph name for what a url opens: the base mark of its kind, the
 * one its door draws. A thing with no icon of its own wears this everywhere
 * it shows (its pin, its tab, the icon picker's preview).
 */
export function kindGlyph(url: string): string {
	if (/^https?:\/\//i.test(url)) return 'wiki';
	if (url.startsWith('/chat')) return 'chats';
	if (url.startsWith('/thread/')) return 'projects';
	if (/^\/(sites?|job|applets?)(\/|$)/.test(url)) return 'applets';
	// A task, linked by `?task=`. A pin made before Tasks joined the
	// Threads door still says /tasks, and opens the door's Tasks view.
	if (url.startsWith('/tasks') || url.startsWith('/threads?view=tasks')) return 'tasks';
	if (url.startsWith('/threads')) return 'projects';
	if (url.startsWith('/wiki')) return 'wiki';
	if (url.startsWith('/day/')) return 'day';
	return 'pages';
}

/** `kindGlyph` as an icon string, for surfaces that take one (tabs, the picker). */
export function kindIcon(url: string): string {
	return `atlas:${kindGlyph(url)}`;
}
