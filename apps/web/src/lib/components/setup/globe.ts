/**
 * The location step's globe: an orthographic sphere of land dots drawn into
 * one canvas, with the chosen place marked and its hour band lit.
 *
 * Why a hand-rolled canvas and not MapLibre's globe: this picker needs no
 * tiles, no labels and no zoom, and MapLibre would bring ~800 KB, a worker
 * and a WebGL context the baked apps' CSP has to allow, all to draw dots. The
 * projection is a dozen lines of trigonometry (below), the land is 4 KB of
 * bundled points (globeLand.ts), and a dot needs no clipping at the horizon:
 * a point on the far side is simply not drawn.
 *
 * Every color is a theme token read off the page (`--color-foreground`,
 * `--color-primary`, `--color-surface`, `--color-border`), resolved to RGB
 * through a 1x1 canvas so any token syntax (hex, oklch, color-mix) works, and
 * re-read when the theme changes. Motion uses Setup's one ease (motion.ts).
 */
import { ease, reducedMotion } from './motion';
import { landPoints } from './globeLand';
import { CITIES, nearestCity, type City } from './cities';

const RAD = Math.PI / 180;
const DEG = 180 / Math.PI;

type RGB = [number, number, number];
interface Palette {
	ink: RGB;
	accent: RGB;
	paper: RGB;
	hair: RGB;
}

interface View {
	lng: number;
	lat: number;
	/** Scale of the sphere, 1 at rest. */
	zoom: number;
	/** Opacity of the whole drawing, for the entrance. */
	alpha: number;
}

interface Tween {
	from: View;
	to: View;
	start: number;
	dur: number;
	/** How far the sphere pulls back mid-flight, for a long hop. */
	hop: number;
}

export interface GlobeTarget {
	lat: number;
	lng: number;
	label: string;
	/** The zone's STANDARD offset from UTC in minutes; lights that hour band.
	 *  Standard, not today's: in summer Chicago is UTC−5, but its hour on the
	 *  globe is the UTC−6 band it sits in. */
	offset: number;
}

export interface GlobeOptions {
	/** A city was chosen on the globe (click or tap). */
	onpick: (c: City) => void;
}

const wrap = (d: number) => ((((d + 180) % 360) + 360) % 360) - 180;
const clamp = (v: number, a: number, b: number) => Math.min(b, Math.max(a, v));
const rgba = (c: RGB, a: number) => `rgba(${c[0]},${c[1]},${c[2]},${a.toFixed(3)})`;

/** The latitude the camera sits at for a place: tilted toward it, never over a pole. */
const tiltFor = (lat: number) => clamp(lat * 0.72, -42, 48);

function readPalette(el: Element): Palette {
	const probe = document.createElement('canvas');
	probe.width = probe.height = 1;
	const pc = probe.getContext('2d', { willReadFrequently: true })!;
	const style = getComputedStyle(el);
	const read = (token: string, fallback: RGB): RGB => {
		const v = style.getPropertyValue(token).trim();
		if (!v) return fallback;
		pc.clearRect(0, 0, 1, 1);
		pc.fillStyle = rgba(fallback, 1);
		pc.fillStyle = v;
		pc.fillRect(0, 0, 1, 1);
		const d = pc.getImageData(0, 0, 1, 1).data;
		return [d[0], d[1], d[2]];
	};
	const ink = read('--color-foreground', [30, 30, 30]);
	const paper = read('--color-surface', [250, 250, 250]);
	return {
		ink,
		paper,
		accent: read('--color-primary', ink),
		hair: read('--color-border', ink),
	};
}

export class Globe {
	private canvas: HTMLCanvasElement;
	private ctx: CanvasRenderingContext2D;
	private opts: GlobeOptions;
	private land: Float32Array;
	/** Per land point: cos(lat), sin(lat), cos(lng), sin(lng). */
	private trig: Float32Array;
	private palette: Palette;
	private size = 0;
	private dpr = 1;
	private view: View = { lng: 0, lat: 20, zoom: 1, alpha: 1 };
	private tween: Tween | null = null;
	private target: GlobeTarget | null = null;
	private hover: City | null = null;
	private raf = 0;
	private born = performance.now();
	private still = reducedMotion();
	private destroyed = false;

	// Dragging and the spin it leaves behind.
	private drag: { id: number; x: number; y: number; moved: number; t: number } | null = null;
	private spin = { lng: 0, lat: 0 };

	private observers: { disconnect(): void }[] = [];

	constructor(canvas: HTMLCanvasElement, opts: GlobeOptions) {
		this.canvas = canvas;
		this.ctx = canvas.getContext('2d')!;
		this.opts = opts;
		this.land = landPoints();
		const n = this.land.length / 2;
		this.trig = new Float32Array(n * 4);
		for (let i = 0; i < n; i++) {
			const la = this.land[i * 2] * RAD;
			const ln = this.land[i * 2 + 1] * RAD;
			this.trig[i * 4] = Math.cos(la);
			this.trig[i * 4 + 1] = Math.sin(la);
			this.trig[i * 4 + 2] = Math.cos(ln);
			this.trig[i * 4 + 3] = Math.sin(ln);
		}
		this.palette = readPalette(canvas);

		const ro = new ResizeObserver(() => this.resize());
		ro.observe(canvas);
		this.observers.push(ro);
		// The theme is an attribute on <html>; a switch repaints in the new ink.
		const mo = new MutationObserver(() => {
			this.palette = readPalette(canvas);
			this.kick();
		});
		mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'class', 'style'] });
		this.observers.push(mo);

		canvas.addEventListener('pointerdown', this.down);
		canvas.addEventListener('pointermove', this.move);
		canvas.addEventListener('pointerup', this.up);
		canvas.addEventListener('pointercancel', this.cancel);
		canvas.addEventListener('pointerleave', this.leave);
		this.resize();
	}

	destroy() {
		this.destroyed = true;
		cancelAnimationFrame(this.raf);
		for (const o of this.observers) o.disconnect();
		const c = this.canvas;
		c.removeEventListener('pointerdown', this.down);
		c.removeEventListener('pointermove', this.move);
		c.removeEventListener('pointerup', this.up);
		c.removeEventListener('pointercancel', this.cancel);
		c.removeEventListener('pointerleave', this.leave);
	}

	/**
	 * Set the place. The first call is the entrance: the sphere turns in from
	 * a third of the world away and settles on it. After that, a change of
	 * place is one eased turn, with a slight pull-back on a long hop so the
	 * eye can follow where it went.
	 */
	setTarget(t: GlobeTarget | null) {
		const first = this.target === null && !this.tweenedOnce;
		const prev = this.target;
		this.target = t;
		// Same place, new name or hour: repaint, but do not turn (and never
		// restart the entrance halfway through).
		if (t && prev && prev.lat === t.lat && prev.lng === t.lng) {
			this.kick();
			return;
		}
		if (!t) {
			this.kick();
			return;
		}
		const to: View = { lng: t.lng, lat: tiltFor(t.lat), zoom: 1, alpha: 1 };
		if (this.still) {
			this.tween = null;
			this.view = to;
			this.kick();
			return;
		}
		if (first) {
			this.tweenedOnce = true;
			this.view = { lng: t.lng - 150, lat: to.lat - 14, zoom: 0.9, alpha: 0 };
			this.tween = { from: { ...this.view }, to, start: performance.now(), dur: 2100, hop: 0 };
		} else {
			const from = { ...this.view };
			const dLng = wrap(to.lng - from.lng);
			const dist = Math.hypot(dLng * Math.cos(to.lat * RAD), to.lat - from.lat);
			to.lng = from.lng + dLng;
			this.tween = {
				from,
				to,
				start: performance.now(),
				dur: clamp(560 + dist * 5.5, 640, 1500),
				hop: clamp(dist / 160, 0, 1) * 0.07,
			};
		}
		this.spin = { lng: 0, lat: 0 };
		this.kick();
	}
	private tweenedOnce = false;

	/** Turn the globe by a few degrees, for the arrow keys. */
	nudge(dLng: number, dLat: number) {
		this.tween = null;
		this.view.lng += dLng;
		this.view.lat = clamp(this.view.lat + dLat, -60, 60);
		this.kick();
	}

	/** The city nearest the middle of the view. */
	centerCity(): City {
		return nearestCity(this.view.lat, this.view.lng);
	}

	// ── projection ───────────────────────────────────────────────────────

	private get radius() {
		return (this.size / 2 - 6) * this.view.zoom;
	}

	/** Screen x, y and depth (z > 0 faces the viewer) for a point in degrees. */
	private project(lat: number, lng: number): [number, number, number] {
		const la = lat * RAD;
		const dl = (lng - this.view.lng) * RAD;
		const p0 = this.view.lat * RAD;
		const cl = Math.cos(la);
		const x = cl * Math.sin(dl);
		const y = Math.cos(p0) * Math.sin(la) - Math.sin(p0) * cl * Math.cos(dl);
		const z = Math.sin(p0) * Math.sin(la) + Math.cos(p0) * cl * Math.cos(dl);
		const r = this.radius;
		const c = this.size / 2;
		return [c + x * r, c - y * r, z];
	}

	/** The inverse: a point on the canvas to degrees, or null off the sphere. */
	private unproject(px: number, py: number): [number, number] | null {
		const r = this.radius;
		const c = this.size / 2;
		const x = (px - c) / r;
		const y = (c - py) / r;
		const rho = x * x + y * y;
		if (rho > 1) return null;
		const z = Math.sqrt(1 - rho);
		const p0 = this.view.lat * RAD;
		const lat = Math.asin(clamp(y * Math.cos(p0) + z * Math.sin(p0), -1, 1)) * DEG;
		const lng = this.view.lng + Math.atan2(x, z * Math.cos(p0) - y * Math.sin(p0)) * DEG;
		return [lat, wrap(lng)];
	}

	// ── input ────────────────────────────────────────────────────────────

	private local(e: PointerEvent): [number, number] {
		const r = this.canvas.getBoundingClientRect();
		return [((e.clientX - r.left) / r.width) * this.size, ((e.clientY - r.top) / r.height) * this.size];
	}

	private down = (e: PointerEvent) => {
		if (e.button !== 0) return;
		const [x, y] = this.local(e);
		if (!this.unproject(x, y)) return;
		this.canvas.setPointerCapture(e.pointerId);
		this.drag = { id: e.pointerId, x, y, moved: 0, t: performance.now() };
		this.tween = null;
		this.spin = { lng: 0, lat: 0 };
		this.view.zoom = 1;
		this.view.alpha = 1;
		this.canvas.dataset.dragging = '';
	};

	private move = (e: PointerEvent) => {
		const [x, y] = this.local(e);
		if (this.drag && e.pointerId === this.drag.id) {
			const dx = x - this.drag.x;
			const dy = y - this.drag.y;
			const now = performance.now();
			const dt = Math.max(1, now - this.drag.t);
			const k = DEG / this.radius;
			this.view.lng -= dx * k;
			this.view.lat = clamp(this.view.lat + dy * k, -60, 60);
			this.spin = { lng: ((-dx * k) / dt) * 16, lat: ((dy * k) / dt) * 16 };
			this.drag.moved += Math.abs(dx) + Math.abs(dy);
			this.drag.x = x;
			this.drag.y = y;
			this.drag.t = now;
			this.hover = null;
			this.kick();
			return;
		}
		const hit = this.cityAt(x, y);
		this.canvas.style.cursor = this.unproject(x, y) ? (hit ? 'pointer' : 'grab') : '';
		if (hit !== this.hover) {
			this.hover = hit;
			this.kick();
		}
	};

	private up = (e: PointerEvent) => {
		if (!this.drag || e.pointerId !== this.drag.id) return;
		const [x, y] = this.local(e);
		const tap = this.drag.moved < 6;
		// A release after a pause is a placement, not a throw.
		if (performance.now() - this.drag.t > 80 || this.still) this.spin = { lng: 0, lat: 0 };
		this.drag = null;
		delete this.canvas.dataset.dragging;
		if (tap) {
			this.spin = { lng: 0, lat: 0 };
			const at = this.unproject(x, y);
			if (at) this.opts.onpick(this.cityAt(x, y) ?? nearestCity(at[0], at[1]));
		}
		this.kick();
	};

	private cancel = (e: PointerEvent) => {
		if (this.drag && e.pointerId === this.drag.id) {
			this.drag = null;
			delete this.canvas.dataset.dragging;
		}
	};

	private leave = () => {
		if (this.hover) {
			this.hover = null;
			this.kick();
		}
	};

	/** The city within a finger's reach of a canvas point, if any. */
	private cityAt(x: number, y: number): City | null {
		let best: City | null = null;
		let bestD = 18;
		for (const c of CITIES) {
			const [cx, cy, z] = this.project(c.lat, c.lng);
			if (z < 0.15) continue;
			const d = Math.hypot(cx - x, cy - y);
			if (d < bestD) {
				bestD = d;
				best = c;
			}
		}
		return best;
	}

	// ── the frame loop ───────────────────────────────────────────────────

	private resize() {
		const rect = this.canvas.getBoundingClientRect();
		const size = Math.round(rect.width);
		if (!size) return;
		this.dpr = Math.min(window.devicePixelRatio || 1, 2);
		this.size = size;
		this.canvas.width = Math.round(size * this.dpr);
		this.canvas.height = Math.round(size * this.dpr);
		this.kick();
	}

	/** Make sure a frame is coming. */
	private kick() {
		if (this.destroyed || this.raf) return;
		this.raf = requestAnimationFrame(this.frame);
	}

	private frame = (now: number) => {
		this.raf = 0;
		if (this.destroyed) return;
		let moving = false;

		if (this.tween) {
			const t = clamp((now - this.tween.start) / this.tween.dur, 0, 1);
			const e = ease(t);
			const { from, to, hop } = this.tween;
			this.view = {
				lng: from.lng + (to.lng - from.lng) * e,
				lat: from.lat + (to.lat - from.lat) * e,
				zoom: from.zoom + (to.zoom - from.zoom) * e - Math.sin(Math.PI * t) * hop,
				alpha: from.alpha + (to.alpha - from.alpha) * Math.min(1, t * 2.4),
			};
			if (t >= 1) {
				this.view = { ...to, lng: wrap(to.lng) };
				this.tween = null;
			}
			moving = true;
		} else if (!this.drag && (Math.abs(this.spin.lng) > 0.01 || Math.abs(this.spin.lat) > 0.01)) {
			this.view.lng += this.spin.lng;
			this.view.lat = clamp(this.view.lat + this.spin.lat, -60, 60);
			this.spin.lng *= 0.94;
			this.spin.lat *= 0.9;
			moving = true;
		}

		this.draw(now);
		// The marker breathes, so the loop runs while a place is shown, unless
		// motion is reduced, where a frame is drawn only when something changed.
		if (moving || (!this.still && this.target)) this.kick();
	};

	private draw(now: number) {
		const { ctx, size, dpr, palette: p } = this;
		if (!size) return;
		const c = size / 2;
		const r = this.radius;
		const A = this.view.alpha;
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, size, size);
		if (A <= 0) return;
		ctx.globalAlpha = 1;

		// The sphere: the paper, a breath darker toward the limb, one hairline.
		const shade = ctx.createRadialGradient(c - r * 0.35, c - r * 0.4, r * 0.1, c, c, r);
		shade.addColorStop(0, rgba(p.ink, 0.012 * A));
		shade.addColorStop(0.75, rgba(p.ink, 0.03 * A));
		shade.addColorStop(1, rgba(p.ink, 0.07 * A));
		ctx.beginPath();
		ctx.arc(c, c, r, 0, Math.PI * 2);
		ctx.fillStyle = shade;
		ctx.fill();

		// The zone's hour band, minutes → degrees. Real zone lines wander off
		// the meridians (Lisbon sits west of its band), so the band is slid
		// just far enough that the place is always inside the lit hour.
		const band = this.target
			? this.target.lng + clamp(wrap(this.target.offset / 4 - this.target.lng), -6, 6)
			: null;
		const bandW = 7.5;

		// The hour band: a lune fifteen degrees wide, the zone's hours on Earth.
		if (band !== null) {
			ctx.beginPath();
			for (let lat = -88; lat < 88; lat += 4) {
				for (let lng = band - bandW; lng < band + bandW - 0.01; lng += 3.75) {
					const q = [
						this.project(lat, lng),
						this.project(lat, lng + 3.75),
						this.project(lat + 4, lng + 3.75),
						this.project(lat + 4, lng),
					];
					if (q.every((v) => v[2] < 0)) continue;
					q.forEach(([x, y, z], i) => {
						if (z < 0) {
							// Pin a point that went round the back to the limb.
							const d = Math.hypot(x - c, y - c) || 1;
							x = c + ((x - c) / d) * r;
							y = c + ((y - c) / d) * r;
						}
						if (i === 0) ctx.moveTo(x, y);
						else ctx.lineTo(x, y);
					});
					ctx.closePath();
				}
			}
			ctx.fillStyle = rgba(p.accent, 0.06 * A);
			ctx.fill();
		}

		// The graticule: the 24 hour meridians and the equator, barely there.
		ctx.lineWidth = 1;
		const meridian = (lng: number, reach = 80) => {
			let pen = false;
			for (let lat = -reach; lat <= reach; lat += 2.5) {
				const [x, y, z] = this.project(lat, lng);
				if (z < 0) {
					pen = false;
					continue;
				}
				if (!pen) ctx.moveTo(x, y);
				else ctx.lineTo(x, y);
				pen = true;
			}
		};
		ctx.beginPath();
		for (let lng = -180; lng < 180; lng += 15) {
			if (band !== null && (Math.abs(wrap(lng - band + bandW)) < 0.01 || Math.abs(wrap(lng - band - bandW)) < 0.01))
				continue;
			meridian(lng);
		}
		{
			let pen = false;
			for (let lng = -180; lng <= 180; lng += 3) {
				const [x, y, z] = this.project(0, lng);
				if (z < 0) {
					pen = false;
					continue;
				}
				if (!pen) ctx.moveTo(x, y);
				else ctx.lineTo(x, y);
				pen = true;
			}
		}
		ctx.strokeStyle = rgba(p.ink, 0.05 * A);
		ctx.stroke();
		if (band !== null) {
			ctx.beginPath();
			meridian(band - bandW, 90);
			meridian(band + bandW, 90);
			ctx.strokeStyle = rgba(p.accent, 0.3 * A);
			ctx.stroke();
		}

		// The land, as dots: darker as they face you, accent inside the band,
		// and brightest in a small pool around the place itself.
		const BUCKETS = 6;
		const inkPaths: Path2D[] = Array.from({ length: BUCKETS }, () => new Path2D());
		const accPaths: Path2D[] = Array.from({ length: BUCKETS }, () => new Path2D());
		const T = this.trig;
		const p0 = this.view.lat * RAD;
		const sp = Math.sin(p0);
		const cp = Math.cos(p0);
		const l0 = this.view.lng * RAD;
		const cl0 = Math.cos(l0);
		const sl0 = Math.sin(l0);
		const dotR = Math.max(0.8, Math.min(1.35, r / 170));
		let tx = 0, ty = 0, tz = 0;
		if (this.target) {
			const la = this.target.lat * RAD;
			const ln = this.target.lng * RAD;
			tx = Math.cos(la) * Math.cos(ln);
			ty = Math.cos(la) * Math.sin(ln);
			tz = Math.sin(la);
		}
		const n = T.length / 4;
		for (let i = 0; i < n; i++) {
			const cla = T[i * 4], sla = T[i * 4 + 1], cln = T[i * 4 + 2], sln = T[i * 4 + 3];
			// cos/sin of (lng - l0)
			const cd = cln * cl0 + sln * sl0;
			const sd = sln * cl0 - cln * sl0;
			const z = sp * sla + cp * cla * cd;
			if (z <= 0.02) continue;
			const x = cla * sd;
			const y = cp * sla - sp * cla * cd;
			const sx = c + x * r;
			const sy = c - y * r;
			let lit = false;
			let near = 0;
			if (band !== null) {
				const lng = this.land[i * 2 + 1];
				lit = Math.abs(wrap(lng - band)) <= bandW;
				// angular closeness to the place, via the dot product
				const dot = cla * cln * tx + cla * sln * ty + sla * tz;
				near = clamp((dot - 0.9925) / 0.0075, 0, 1);
			}
			const depth = Math.min(1, z * 1.25);
			const level = Math.min(BUCKETS - 1, Math.floor((depth * 0.72 + near * 0.5) * BUCKETS));
			const path = lit || near > 0 ? accPaths[level] : inkPaths[level];
			path.moveTo(sx + dotR, sy);
			path.arc(sx, sy, dotR * (0.75 + 0.25 * depth), 0, Math.PI * 2);
		}
		for (let b = 0; b < BUCKETS; b++) {
			const k = (b + 0.5) / BUCKETS;
			ctx.fillStyle = rgba(p.ink, (0.1 + 0.34 * k) * A);
			ctx.fill(inkPaths[b]);
			ctx.fillStyle = rgba(p.accent, (0.28 + 0.62 * k) * A);
			ctx.fill(accPaths[b]);
		}

		// The limb: one hairline in the border token.
		ctx.beginPath();
		ctx.arc(c, c, r, 0, Math.PI * 2);
		ctx.strokeStyle = rgba(p.hair, A);
		ctx.lineWidth = 1;
		ctx.stroke();

		// A city under the pointer: a ring and its name.
		if (this.hover && (!this.target || this.hover.name !== this.target.label)) {
			const [hx, hy, hz] = this.project(this.hover.lat, this.hover.lng);
			if (hz > 0) {
				ctx.beginPath();
				ctx.arc(hx, hy, 4, 0, Math.PI * 2);
				ctx.strokeStyle = rgba(p.ink, 0.7 * A);
				ctx.lineWidth = 1.5;
				ctx.stroke();
				this.label(this.hover.name, hx, hy - 12, 13, rgba(p.ink, 0.72 * A));
			}
		}

		// The place: a filled dot, a halo, and a ring that breathes outward.
		if (this.target) {
			const [mx, my, mz] = this.project(this.target.lat, this.target.lng);
			if (mz > 0) {
				const f = clamp(mz * 4, 0, 1) * A;
				if (!this.still) {
					const period = 2600;
					for (const lag of [0, period / 2]) {
						const ph = ((now - this.born + lag) % period) / period;
						const e = 1 - (1 - ph) ** 3;
						ctx.beginPath();
						ctx.arc(mx, my, 7 + e * 22, 0, Math.PI * 2);
						ctx.strokeStyle = rgba(p.accent, 0.45 * (1 - ph) * f);
						ctx.lineWidth = 1.5;
						ctx.stroke();
					}
				}
				ctx.beginPath();
				ctx.arc(mx, my, 13, 0, Math.PI * 2);
				ctx.fillStyle = rgba(p.accent, 0.14 * f);
				ctx.fill();
				ctx.beginPath();
				ctx.arc(mx, my, 5.5, 0, Math.PI * 2);
				ctx.fillStyle = rgba(p.accent, f);
				ctx.fill();
				ctx.lineWidth = 2.5;
				ctx.strokeStyle = rgba(p.paper, f);
				ctx.stroke();
			}
		}
		ctx.globalAlpha = 1;
	}

	private label(text: string, x: number, y: number, px: number, fill: string) {
		const { ctx, palette: p } = this;
		const serif = getComputedStyle(this.canvas).getPropertyValue('--font-serif').trim() || 'Georgia, serif';
		ctx.font = `400 ${px}px ${serif}`;
		ctx.textAlign = 'center';
		ctx.textBaseline = 'alphabetic';
		ctx.lineJoin = 'round';
		ctx.lineWidth = 4;
		ctx.strokeStyle = rgba(p.paper, 0.92);
		ctx.strokeText(text, x, y);
		ctx.fillStyle = fill;
		ctx.fillText(text, x, y);
	}
}
