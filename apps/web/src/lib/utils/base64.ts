/**
 * Base64 of `bytes`, a chunk at a time: spreading a whole page's Yjs state
 * into one `String.fromCharCode` call passes the engine's limit on a call's
 * arguments (about 120 KB in V8, less in Safari) and throws.
 */
export function bytesToBase64(bytes: Uint8Array): string {
	let out = '';
	for (let i = 0; i < bytes.length; i += 0x8000) out += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
	return btoa(out);
}
