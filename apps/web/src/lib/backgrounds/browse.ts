/**
 * Moving through pictures from the pickers: the one before or after, and a
 * thumbnail. Kept apart from resolve.ts, which the home picture needs at
 * launch, so this loads with the pickers instead.
 */
import { filesOf, formatAddress, type Address, type Variant } from './resolve';

/** The narrowest file, AVIF before WebP, for a thumbnail. */
export function narrowest(v: Variant, url: (rel: string) => string): string {
	return url(filesOf(v)[0].rel);
}

/**
 * The picture before or after `current` in `items`, wrapping at either end.
 * From a picture that is not in `items`, the first or the last.
 */
export function stepFrom(items: Address[], current: string, dir: 1 | -1): Address | undefined {
	const n = items.length;
	if (n === 0) return undefined;
	const i = items.findIndex((a) => formatAddress(a) === current);
	if (i < 0) return dir === 1 ? items[0] : items[n - 1];
	return items[(i + dir + n) % n];
}
