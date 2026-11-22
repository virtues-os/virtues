/**
 * Drawing the dots style: one frame of the field at a moment, so the moving
 * background (components/backgrounds/DotsField.svelte) and the pickers'
 * thumbnails draw the same thing. Pure: the caller owns the canvas, the
 * clock and the color.
 *
 * Dots sit on a grid and move around their place, never across the screen,
 * so a frame is cheap to draw and the field reads as one surface. They are
 * fainter toward the top, so the field rises out of the bottom of the window
 * as the paintings do.
 */
import type { DotsParams } from './resolve';

/** How many opacity steps a frame is drawn in: one path per step. */
const STEPS = 6;

/** A stable number in [0, 1) for a grid cell, so each dot keeps its own phase. */
function cellPhase(i: number, j: number): number {
	let h = Math.imul(i, 374761393) + Math.imul(j, 668265263);
	h = Math.imul(h ^ (h >>> 13), 1274126177);
	return ((h ^ (h >>> 16)) >>> 0) / 4294967296;
}

/**
 * Draw one frame into a `width` × `height` area (CSS pixels; the caller has
 * scaled the context for the device). `t` is in seconds, `ink` a CSS color,
 * and `opacity` the darkest a dot gets.
 */
export function drawDots(
	ctx: CanvasRenderingContext2D,
	width: number,
	height: number,
	p: DotsParams,
	t: number,
	ink: string,
	opacity: number,
): void {
	ctx.clearRect(0, 0, width, height);
	const cols = Math.ceil(width / p.gap) + 2;
	const rows = Math.ceil(height / p.gap) + 2;
	const turn = (2 * Math.PI * t) / p.period;
	const paths = Array.from({ length: STEPS }, () => new Path2D());

	for (let j = 0; j < rows; j++) {
		for (let i = 0; i < cols; i++) {
			const ph = cellPhase(i, j);
			const x0 = (i - 1) * p.gap;
			const y0 = (j - 1) * p.gap;
			let x = x0;
			let y = y0;
			let r = p.radius;
			let a = 1;
			if (p.motion === 'drift') {
				x += p.reach * Math.sin(turn + ph * 6.283 + y0 * 0.011);
				y += p.reach * Math.cos(turn * 0.7 + ph * 6.283 + x0 * 0.007);
			} else if (p.motion === 'swell') {
				// A slow wave crosses the field: dots grow and brighten as it passes.
				const s = 0.5 + 0.5 * Math.sin(turn - x0 * 0.006 - y0 * 0.004);
				r *= 0.55 + 0.75 * s;
				a = 0.35 + 0.65 * s;
				x += p.reach * Math.sin(turn + ph * 6.283);
			} else {
				// Each column falls at its own pace and wraps, swaying a little.
				const span = height + 2 * p.gap;
				const fall = (t / p.period) * span * (0.6 + 0.4 * ph);
				y = ((((y0 + fall) % span) + span) % span) - p.gap;
				x += p.reach * Math.sin(turn * 0.5 + ph * 6.283);
			}
			// Fainter toward the top.
			a *= 0.25 + 0.75 * Math.min(1, Math.max(0, y / height));
			const step = Math.min(STEPS - 1, Math.floor(a * STEPS));
			if (step <= 0) continue;
			paths[step].moveTo(x + r, y);
			paths[step].arc(x, y, r, 0, 2 * Math.PI);
		}
	}

	ctx.fillStyle = ink;
	for (let step = 1; step < STEPS; step++) {
		ctx.globalAlpha = (opacity * (step + 0.5)) / STEPS;
		ctx.fill(paths[step]);
	}
	ctx.globalAlpha = 1;
}

/** How dark the darkest dot gets in each appearance. */
export function dotsOpacity(dark: boolean): number {
	return dark ? 0.42 : 0.32;
}

const previews = new Map<string, string>();

/**
 * A still of a dots picture as a `data:` URL, for a picker card: drawn on
 * the desk color, in the current theme. Cached per picture
 * and theme.
 */
export function dotsPreview(id: string, p: DotsParams, dark: boolean, width = 320, height = 180): string {
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
	// A card is small, so its dots are larger and darker than the field's to read at all.
	const card = { ...p, gap: p.gap * 0.8, radius: p.radius * 1.5, reach: p.reach * 0.75 };
	drawDots(ctx, width, height, card, p.period * 0.3, ink, Math.min(0.95, dotsOpacity(dark) * 2.4));
	ctx.globalCompositeOperation = 'destination-over';
	ctx.fillStyle = desk;
	ctx.fillRect(0, 0, width, height);
	const url = canvas.toDataURL('image/png');
	previews.set(key, url);
	return url;
}
