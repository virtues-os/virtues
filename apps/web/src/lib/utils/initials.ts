/**
 * Up to two letters for a name, drawn where there is no picture: the first
 * and last words' first letters, or the first two letters of a one-word name.
 * Letters are counted by code point, so a name that starts with a character
 * outside the Basic Multilingual Plane is not cut in half.
 */
export function initials(name: string): string {
	const words = name.trim().split(/\s+/).filter(Boolean);
	if (!words.length) return "?";
	if (words.length === 1) return Array.from(words[0]).slice(0, 2).join("").toUpperCase();
	const first = Array.from(words[0])[0];
	const last = Array.from(words[words.length - 1])[0];
	return (first + last).toUpperCase();
}
