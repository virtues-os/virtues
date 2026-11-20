/**
 * The background setting, resolved to a picture.
 *
 * A setting is one path that can stop at any depth: `plain`, a style
 * (`watercolors`), a collection in it (`watercolors/forest`), or one
 * picture (`watercolors/forest/birch-grove`). The empty setting is the
 * default, the bundled lake. A path that stops above a picture rotates daily
 * through every picture under it, shuffled, with no repeat until each has
 * shown. Light and Dark are versions of a picture, never part of the path.
 * See agents/plan/backgrounds-plan.md; the manifests come from
 * tools/backgrounds/build.py through GET /api/backgrounds.
 */

export type Appearance = 'light' | 'dark';

export interface Variant {
	source: string;
	title: string;
	/** Mean relative luminance of the painted pixels, 0 to 1. */
	luminance: number;
	/** [x, y, w, h] as fractions: roughly where the subject is. */
	focus: [number, number, number, number];
	/** The edge left unpainted (transparent), if any. */
	fade: 'top' | 'bottom' | 'none';
	width: number;
	height: number;
	/** Format, then width as a string, then the path under /api/backgrounds/files/<style>/. */
	files: { avif: Record<string, string>; webp: Record<string, string> };
}

/** A picture of an image style: one file set per appearance. */
export interface ImagePicture {
	id: string;
	light: Variant;
	dark: Variant;
}

/**
 * A picture of the dots style: how the field moves. The app draws it on a
 * canvas in the current theme's ink (components/backgrounds/DotsField.svelte),
 * so one picture serves Light and Dark.
 */
export interface DotsPicture {
	id: string;
	title: string;
	dots: DotsParams;
}

export interface DotsParams {
	/** How the field moves: drifting on a slow current, swelling in and out, or falling. */
	motion: 'drift' | 'swell' | 'rain';
	/** Space between dots on the grid, in CSS pixels. */
	gap: number;
	/** Dot radius, in CSS pixels. */
	radius: number;
	/** How far a dot wanders from its place, in CSS pixels. */
	reach: number;
	/** Seconds for one slow cycle of the motion. */
	period: number;
}

export type Picture = ImagePicture | DotsPicture;

export function isDots(p: Picture): p is DotsPicture {
	return 'dots' in p;
}

/** A picture's name in the current appearance. */
export function pictureTitle(p: Picture, appearance: Appearance): string {
	return isDots(p) ? p.title : p[appearance].title;
}

export interface Collection<P extends Picture = Picture> {
	id: string;
	title: string;
	pictures: P[];
}

interface StyleBase {
	version: 1;
	style: string;
	/** The style's name in the pickers. */
	title: string;
}

/** A style of pictures, installed on the server (tools/backgrounds/build.py). */
export interface ImageStyle extends StyleBase {
	renderer: 'image';
	collections: Collection<ImagePicture>[];
}

/** A style drawn by the app itself, shipped with it (lib/backgrounds/builtin.ts). */
export interface DotsStyle extends StyleBase {
	renderer: 'dots';
	collections: Collection<DotsPicture>[];
}

export type StyleManifest = ImageStyle | DotsStyle;

/** A style the server can download (GET /api/backgrounds, `available`). */
export interface AvailableStyle {
	style: string;
	title: string;
	bytes: number;
	/** A small picture per appearance, as `data:` URLs. */
	preview: Partial<Record<Appearance, string>>;
}

/** A download in progress (GET /api/backgrounds, `installing`). */
export interface DownloadProgress {
	received_bytes: number;
	total_bytes: number;
}

export interface Address {
	style: string;
	collection: string;
	picture: string;
}

export type Setting =
	| { kind: 'default' }
	| { kind: 'plain' }
	| { kind: 'path'; style: string; collection?: string; picture?: string };

const SEGMENT = /^[a-z0-9-]{1,64}$/;

/** Read a stored setting. Anything malformed is the default. */
export function parseSetting(raw: unknown): Setting {
	if (typeof raw !== 'string' || raw === '') return { kind: 'default' };
	if (raw === 'plain') return { kind: 'plain' };
	const parts = raw.split('/');
	if (parts.length > 3 || !parts.every((p) => SEGMENT.test(p))) return { kind: 'default' };
	const [style, collection, picture] = parts;
	return { kind: 'path', style, collection, picture };
}

export function formatAddress(a: Address): string {
	return `${a.style}/${a.collection}/${a.picture}`;
}

/** Every picture the setting covers, in manifest order. Empty when nothing matches. */
export function pool(styles: StyleManifest[], setting: Setting): Address[] {
	if (setting.kind !== 'path') return [];
	const out: Address[] = [];
	for (const s of styles) {
		if (s.style !== setting.style) continue;
		for (const c of s.collections) {
			if (setting.collection && c.id !== setting.collection) continue;
			for (const p of c.pictures) {
				if (setting.picture && p.id !== setting.picture) continue;
				out.push({ style: s.style, collection: c.id, picture: p.id });
			}
		}
	}
	return out;
}

export function find(styles: StyleManifest[], a: Address): Picture | undefined {
	return styles
		.find((s) => s.style === a.style)
		?.collections.find((c) => c.id === a.collection)
		?.pictures.find((p) => p.id === a.picture);
}

/** Whole local days since 1970-01-01, so the picture changes at local midnight. */
export function dayNumber(date: Date): number {
	return Math.floor(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86_400_000);
}

function hash(text: string): number {
	let h = 2166136261;
	for (let i = 0; i < text.length; i++) {
		h ^= text.charCodeAt(i);
		h = Math.imul(h, 16777619);
	}
	return h >>> 0;
}

function random(seed: number): () => number {
	let t = seed;
	return () => {
		t = (t + 0x6d2b79f5) >>> 0;
		let r = Math.imul(t ^ (t >>> 15), 1 | t);
		r ^= r + Math.imul(r ^ (r >>> 7), 61 | r);
		return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
	};
}

/** The order a cycle walks the pool in: a shuffle seeded by the setting and the cycle. */
function cycleOrder(n: number, key: string, cycle: number): number[] {
	const order = Array.from({ length: n }, (_, i) => i);
	const next = random(hash(`${key}:${cycle}`));
	for (let i = n - 1; i > 0; i--) {
		const j = Math.floor(next() * (i + 1));
		[order[i], order[j]] = [order[j], order[i]];
	}
	return order;
}

/**
 * The picture for a day. Days are grouped into cycles as long as the pool;
 * each cycle walks its own shuffle of the pool, so a picture shows once per
 * cycle. `key` (the setting) seeds the shuffle, so two settings over the same
 * pictures do not walk in step. A cycle never opens with the picture the one
 * before it ended on, so no picture shows two days running.
 */
export function pickForDay<T>(items: T[], key: string, day: number): T | undefined {
	const n = items.length;
	if (n === 0) return undefined;
	if (n === 1) return items[0];
	// Two pictures can only alternate without a repeat.
	if (n === 2) return items[(((day + hash(key)) % 2) + 2) % 2];
	const cycle = Math.floor(day / n);
	const order = cycleOrder(n, key, cycle);
	if (order[0] === cycleOrder(n, key, cycle - 1)[n - 1]) [order[0], order[1]] = [order[1], order[0]];
	return items[order[((day % n) + n) % n]];
}

/** A `srcset` for one format of a variant, widest last. */
export function srcset(v: Variant, format: 'avif' | 'webp', url: (rel: string) => string): string {
	return Object.entries(v.files[format])
		.map(([w, rel]) => ({ w: Number(w), rel }))
		.sort((a, b) => a.w - b.w)
		.map(({ w, rel }) => `${url(rel)} ${w}w`)
		.join(', ');
}

/** The widest WebP, for the `<img>` fallback. */
export function widest(v: Variant, url: (rel: string) => string): string {
	const [, rel] = Object.entries(v.files.webp).sort((a, b) => Number(b[0]) - Number(a[0]))[0];
	return url(rel);
}

