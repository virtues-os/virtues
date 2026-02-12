/**
 * Check if a value is an emoji (vs a Remix Icon name like "ri:folder-line").
 * Emojis are raw Unicode characters and don't contain ":".
 */
export function isEmoji(val: string): boolean {
	return !val.includes(':');
}
