/**
 * Words that stand straight on the background take the ink that reads over
 * the part of the picture behind them. A picture that fills the window can
 * be a pale sky behind the headline and a dark wood behind the notes line,
 * so each element is measured on its own.
 *
 * `use:onScene` sets `data-on-scene` to `light` or `dark`, which gives the
 * element the `--on-scene*` colors (themes.css) once the picture has loaded.
 * It sets nothing over the plain desk or a background that fades up from
 * the bottom: the theme's own ink reads over those.
 */
import { background } from '$lib/stores/background.svelte';
import { DIM_BRIGHTNESS, inkOver, type SceneInk } from './resolve';

/** A picture that fills the window, as much of it as measuring needs. */
interface FullPicture {
	key: string;
	sample: string;
	dim: boolean;
	focus: [number, number];
	/** The ink to use when the picture cannot be measured. */
	ink: SceneInk;
}

/** What fills the window now, or null when nothing does. */
function fullPicture(): FullPicture | null {
	const shown = background.arrived;
	if (shown.kind === 'picture') return shown.strip ? null : shown;
	if (shown.kind !== 'lake') return null;
	const dim = background.appearance === 'dark';
	// The focus is the `position` Background.svelte is given.
	return { key: `lake:${dim}`, sample: '/scenes/lake.avif', dim, focus: [0.7, 1], ink: dim ? 'light' : 'dark' };
}

/** A small copy of the picture, as relative luminance per pixel. */
interface Sample {
	width: number;
	height: number;
	luminance: Float32Array;
}

const SAMPLE_WIDTH = 160;

let latest: { key: string; sample: Promise<Sample | null> } | null = null;

/** The picture's sample. Only the picture on screen is kept. */
function sampleOf(p: FullPicture): Promise<Sample | null> {
	if (latest?.key !== p.key) latest = { key: p.key, sample: read(p) };
	return latest.sample;
}

/** Read through `fetch`: an `<img>` from the app's own server origin would taint the canvas. */
async function read(p: FullPicture): Promise<Sample | null> {
	try {
		const res = await fetch(p.sample);
		if (!res.ok) return null;
		const bitmap = await createImageBitmap(await res.blob());
		const width = SAMPLE_WIDTH;
		const height = Math.max(1, Math.round((width * bitmap.height) / bitmap.width));
		const canvas = document.createElement('canvas');
		canvas.width = width;
		canvas.height = height;
		const ctx = canvas.getContext('2d', { willReadFrequently: true });
		if (!ctx) return null;
		ctx.drawImage(bitmap, 0, 0, width, height);
		bitmap.close();
		const { data } = ctx.getImageData(0, 0, width, height);
		const scale = (p.dim ? DIM_BRIGHTNESS : 1) / 255;
		const linear = (c: number) => (c * scale <= 0.04045 ? (c * scale) / 12.92 : ((c * scale + 0.055) / 1.055) ** 2.4);
		const luminance = new Float32Array(width * height);
		for (let i = 0; i < luminance.length; i++) {
			luminance[i] = 0.2126 * linear(data[i * 4]) + 0.7152 * linear(data[i * 4 + 1]) + 0.0722 * linear(data[i * 4 + 2]);
		}
		return { width, height, luminance };
	} catch (error) {
		// The picture's own luminance decides instead.
		console.warn('Background not measured:', error);
		return null;
	}
}

/** Mean luminance of the picture behind `node`, under the scene's cover crop. */
function luminanceBehind(node: HTMLElement, s: Sample, focus: [number, number]): number | null {
	const scene = document.querySelector('.window-scene, .home-scene')?.getBoundingClientRect();
	const box = node.getBoundingClientRect();
	if (!scene?.width || !scene.height || !box.width || !box.height) return null;
	const scale = Math.max(scene.width / s.width, scene.height / s.height);
	const left = (scene.width - s.width * scale) * focus[0];
	const top = (scene.height - s.height * scale) * focus[1];
	const clamp = (v: number, max: number) => Math.min(max, Math.max(0, v));
	const x0 = clamp(Math.floor((box.left - scene.left - left) / scale), s.width - 1);
	const x1 = clamp(Math.ceil((box.right - scene.left - left) / scale), s.width);
	const y0 = clamp(Math.floor((box.top - scene.top - top) / scale), s.height - 1);
	const y1 = clamp(Math.ceil((box.bottom - scene.top - top) / scale), s.height);
	let sum = 0;
	let n = 0;
	for (let y = y0; y < y1; y++) for (let x = x0; x < x1; x++, n++) sum += s.luminance[y * s.width + x];
	return n ? sum / n : null;
}

export function onScene(node: HTMLElement): void {
	$effect(() => {
		const picture = fullPicture();
		if (!picture) {
			delete node.dataset.onScene;
			return;
		}
		let stopped = false;
		const measure = async () => {
			const sample = await sampleOf(picture);
			if (stopped) return;
			const luminance = sample && luminanceBehind(node, sample, picture.focus);
			node.dataset.onScene = luminance === null ? picture.ink : inkOver(luminance);
		};
		void measure();
		const resized = new ResizeObserver(() => void measure());
		resized.observe(node);
		const onResize = () => void measure();
		window.addEventListener('resize', onResize);
		return () => {
			stopped = true;
			resized.disconnect();
			window.removeEventListener('resize', onResize);
		};
	});
}
