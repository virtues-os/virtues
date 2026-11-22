/**
 * Drawing the ASCII style: a scene drawn as characters in a monospace grid,
 * one frame at a moment, so the moving background
 * (components/backgrounds/AsciiField.svelte) and the pickers' thumbnails
 * draw the same thing. Pure: the caller owns the canvas, the clock and the
 * color, and every scene is a function of time alone, so a frame needs no
 * state from the one before.
 *
 * A scene gives each cell an ink level from 0 to 1, and the level picks a
 * character from a ramp of denser ones. Levels under GROUND stay blank, so
 * only the subject and a little of its water or sky show.
 */
import type { AsciiParams, AsciiScene } from './resolve';

/** Characters from faint to dense. */
const RAMP = ' .:-=+*#%@';
/** Ink below this level draws nothing. */
const GROUND = 0.1;
/** A cell is this many times taller than the font size. */
const LINE = 1.05;

const clamp = (x: number, a = 0, b = 1) => (x < a ? a : x > b ? b : x);
const smooth = (a: number, b: number, x: number) => {
	x = clamp((x - a) / (b - a));
	return x * x * (3 - 2 * x);
};

/** A stable number in [0, 1) for a pair of integers. */
function hash(x: number, y: number): number {
	let h = (Math.imul(x | 0, 374761393) + Math.imul(y | 0, 668265263)) | 0;
	h = Math.imul(h ^ (h >>> 13), 1274126177);
	return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

function noise(x: number, y: number): number {
	const xi = Math.floor(x);
	const yi = Math.floor(y);
	const xf = x - xi;
	const yf = y - yi;
	const u = xf * xf * (3 - 2 * xf);
	const v = yf * yf * (3 - 2 * yf);
	const a = hash(xi, yi);
	const b = hash(xi + 1, yi);
	const c = hash(xi, yi + 1);
	const d = hash(xi + 1, yi + 1);
	return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
}

function fbm(x: number, y: number, octaves = 4): number {
	let sum = 0;
	let amp = 0.5;
	let f = 1;
	for (let i = 0; i < octaves; i++) {
		sum += amp * noise(x * f, y * f);
		f *= 2.03;
		amp *= 0.5;
	}
	return sum / (1 - Math.pow(0.5, octaves));
}

interface Body {
	/** Half-thickness at the widest, as a share of the length. */
	thick: number;
	/** How far the tail fin spreads, as a share of the length. */
	tail: number;
	/** Dorsal fin height, as a share of the length; 0 for none. */
	dorsal: number;
	/** Pectoral fin length, as a share of the length; 0 for none. */
	pec: number;
	/** How fast the body waves, in radians a second. */
	wig: number;
}

/**
 * Ink for a swimming body at (u, v): nose toward `angle`, tail behind, a
 * wave running down its length. Coordinates are in units of the field's
 * height.
 */
function body(u: number, v: number, cx: number, cy: number, angle: number, len: number, t: number, o: Body): number {
	const dx = u - cx;
	const dy = v - cy;
	const c = Math.cos(angle);
	const s0 = Math.sin(angle);
	const lx = dx * c + dy * s0;
	let ly = -dx * s0 + dy * c;
	// 0 at the nose, 1 at the root of the tail.
	const s = (len * 0.5 - lx) / len;
	if (s < -0.02 || s > 1.12) return 0;
	ly -= Math.sin(s * 3.2 - t * o.wig) * 0.04 * len * s * s;
	const h = len * o.thick * Math.pow(Math.sin(Math.PI * Math.min(clamp(s) * 1.08, 1)), 0.75);
	let b = 0;
	if (s >= 0 && Math.abs(ly) < h) b = 0.5 + 0.45 * (1 - Math.abs(ly) / h) * (ly > 0 ? 0.75 : 1);
	if (s > 0.82) {
		const k = (s - 0.82) / 0.3;
		const spread = len * o.tail * k;
		if (Math.abs(ly) < spread && !(k > 0.55 && Math.abs(ly) < spread * 0.35)) b = Math.max(b, 0.62);
	}
	if (o.dorsal && s > 0.3 && s < 0.5) {
		const rise = s < 0.38 ? (s - 0.3) / 0.08 : (0.5 - s) / 0.12;
		if (ly < -h * 0.85 && ly > -h - len * o.dorsal * rise) b = Math.max(b, 0.75);
	}
	if (o.pec && s > 0.22 && s < 0.42) {
		const k = (s - 0.22) / 0.2;
		if (ly > h * 0.6 && ly < h + len * o.pec * (1 - k) * Math.min(1, k * 4)) b = Math.max(b, 0.6);
	}
	return b;
}

/** Where a swimmer is along a lap across the field and back off the edge. */
function lap(x: number, width: number, margin: number): number {
	const w = width + 2 * margin;
	return ((((x + margin) % w) + w) % w) - margin;
}

const SHARKS = [
	{ y: 0.32, len: 0.55, speed: 0.135, phase: 0.1, dir: 1 },
	{ y: 0.62, len: 0.38, speed: 0.18, phase: 0.9, dir: -1 },
	{ y: 0.82, len: 0.25, speed: 0.24, phase: 0.4, dir: 1 },
];
const SHARK: Body = { thick: 0.11, tail: 0.2, dorsal: 0.13, pec: 0.12, wig: 4 };
const WHALE: Body = { thick: 0.13, tail: 0.28, dorsal: 0.025, pec: 0.22, wig: 1.1 };
const BUBBLES = 30;

type Field = (u: number, v: number, t: number, aspect: number) => number;

const FIELDS: Record<AsciiScene, Field> = {
	// Three sharks cross at different depths under rays of light.
	sharks(u, v, t, aspect) {
		// Light from the surface, faint enough that only its brightest flecks show.
		let b =
			0.03 +
			0.07 * Math.max(0, Math.sin(u * 5 + fbm(u * 2, t * 0.05) * 3 + t * 0.15)) * (1 - v) +
			0.04 * fbm(u * 6 + t * 0.2, v * 6, 2);
		for (const k of SHARKS) {
			const x = lap(k.phase * aspect + k.dir * t * k.speed, aspect, k.len);
			const y = k.y + Math.sin(t * 0.6 + k.phase * 9) * 0.01;
			b = Math.max(b, body(u, v, x, y, k.dir > 0 ? 0 : Math.PI, k.len, t, SHARK));
		}
		return b;
	},
	// One humpback crossing very slowly.
	whale(u, v, t, aspect) {
		const x = lap(0.2 * aspect + t * 0.025, aspect, 0.9);
		const water = 0.05 + 0.08 * (1 - v) * fbm(u * 3 + t * 0.03, v * 2, 3);
		return Math.max(water, body(u, v, x, 0.55 + Math.sin(t * 0.2) * 0.03, -0.05, 0.95, t, WHALE));
	},
	// A cratered moon turning, its lit side moving across it.
	moon(u, v, t, aspect) {
		const x = (u - aspect / 2) / 0.38;
		const y = (v - 0.5) / 0.38;
		const q = x * x + y * y;
		if (q > 1) return hash(Math.floor(u * 140), Math.floor(v * 90)) > 0.994 ? 0.45 : 0;
		const z = Math.sqrt(1 - q);
		const lon = Math.atan2(x, z) + t * 0.06;
		const lat = Math.asin(y);
		const ground = fbm(lon * 2.2 + 9, lat * 2.2, 5);
		const craters = smooth(0.55, 0.62, fbm(lon * 5, lat * 5, 3));
		const a = t * 0.03;
		const lx = Math.cos(a) * 0.8;
		const ly = -0.25;
		const lz = Math.sin(a) * 0.5 + 0.55;
		const lit = (x * lx + y * ly + z * lz) / Math.hypot(lx, ly, lz);
		return clamp(clamp(lit * 1.4 + 0.05) * (0.55 + 0.5 * ground - 0.25 * craters) + 0.04);
	},
};

/** Characters a scene puts on top of its field, such as the whale's bubbles. */
function marks(scene: AsciiScene, t: number, aspect: number, put: (u: number, v: number, ch: string) => void): void {
	if (scene !== 'whale') return;
	for (let i = 0; i < BUBBLES; i++) {
		const x = hash(i, 1);
		const rise = 0.3 + hash(i, 2);
		const y = 1 - ((hash(i, 3) + t * 0.04 * rise) % 1.1);
		put(x * aspect + Math.sin(t + i) * 0.01, y, rise > 0.9 ? 'o' : '°');
	}
}

/**
 * Draw one frame into a `width` × `height` area (CSS pixels; the caller has
 * scaled the context for the device). `t` is in seconds, `ink` a CSS color,
 * and `opacity` how strong the characters are.
 */
export function drawAscii(
	ctx: CanvasRenderingContext2D,
	width: number,
	height: number,
	p: AsciiParams,
	t: number,
	ink: string,
	opacity: number,
): void {
	ctx.clearRect(0, 0, width, height);
	ctx.font = `${p.cell}px ui-monospace, "SF Mono", SFMono-Regular, Menlo, monospace`;
	ctx.textBaseline = 'top';
	const cw = ctx.measureText('M').width;
	const ch = p.cell * LINE;
	const cols = Math.max(4, Math.floor(width / cw));
	const rows = Math.max(4, Math.floor(height / ch));
	const fieldHeight = rows * ch;
	const aspect = (cols * cw) / fieldHeight;
	const time = t * p.speed;
	const field = FIELDS[p.scene];
	const n = RAMP.length - 1;
	const grid: string[][] = [];

	for (let r = 0; r < rows; r++) {
		const row = new Array<string>(cols);
		const v = (r + 0.5) / rows;
		for (let c = 0; c < cols; c++) {
			const b = field(((c + 0.5) * cw) / fieldHeight, v, time, aspect);
			row[c] = b < GROUND ? ' ' : RAMP[Math.max(1, Math.round(clamp((b - GROUND) / (1 - GROUND)) * n))];
		}
		grid.push(row);
	}
	marks(p.scene, time, aspect, (u, v, mark) => {
		const c = Math.floor((u * fieldHeight) / cw);
		const r = Math.floor(v * rows);
		if (r >= 0 && r < rows && c >= 0 && c < cols) grid[r][c] = mark;
	});

	ctx.fillStyle = ink;
	ctx.globalAlpha = opacity;
	const ox = (width - cols * cw) / 2;
	const oy = (height - fieldHeight) / 2;
	for (let r = 0; r < rows; r++) ctx.fillText(grid[r].join(''), ox, oy + r * ch);
	ctx.globalAlpha = 1;
}

/** How strong the characters are in each appearance. */
export function asciiOpacity(dark: boolean): number {
	return dark ? 0.5 : 0.42;
}

/** The moment a still frame shows: the sharks and the whale in view, the moon half lit. */
export const ASCII_STILL_AT = 8;

const previews = new Map<string, string>();

/**
 * A still of an ASCII picture as a `data:` URL, for a picker card: drawn on
 * the desk color, in the current theme. Cached per picture and theme.
 */
export function asciiPreview(id: string, p: AsciiParams, dark: boolean, width = 320, height = 180): string {
	const key = `${id}:${dark ? 'dark' : 'light'}`;
	const cached = previews.get(key);
	if (cached) return cached;
	if (typeof document === 'undefined') return '';
	const canvas = document.createElement('canvas');
	canvas.width = width;
	canvas.height = height;
	const ctx = canvas.getContext('2d');
	if (!ctx) return '';
	const styles = getComputedStyle(document.documentElement);
	const desk = styles.getPropertyValue('--desk').trim() || (dark ? '#141416' : '#f4f4f4');
	const ink = styles.getPropertyValue('--color-foreground').trim() || (dark ? '#ededef' : '#1a2030');
	// A card is small, so its characters are smaller and darker than the field's to read at all.
	drawAscii(ctx, width, height, { ...p, cell: 6 }, ASCII_STILL_AT / p.speed, ink, Math.min(0.95, asciiOpacity(dark) * 1.8));
	ctx.globalCompositeOperation = 'destination-over';
	ctx.fillStyle = desk;
	ctx.fillRect(0, 0, width, height);
	const url = canvas.toDataURL('image/png');
	previews.set(key, url);
	return url;
}
