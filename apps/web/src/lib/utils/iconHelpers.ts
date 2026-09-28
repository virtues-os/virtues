/**
 * Check if a value is an emoji (vs a Remix Icon name like "ri:folder-line").
 * Emojis are raw Unicode characters and don't contain ":".
 */
export function isEmoji(val: string): boolean {
	return !val.includes(':');
}

/**
 * A project's glyph when it has none of its own: Atlas's closed book, the one
 * the sidebar rows beside it are drawn from. Every surface that shows a
 * project reads this, so the tab, the row, the menu and the trash agree.
 */
export const PROJECT_ICON = 'atlas:projects';
