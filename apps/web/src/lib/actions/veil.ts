/**
 * `use:veiled` — hide a passage's names and hard phrases behind drifting ink.
 *
 * What is hidden inside the node:
 * - every entity link the markdown renders (`.ref-link`: people, places);
 * - every phrase in `phrases` (the spans the writer marked with ⟦ ⟧);
 * - the whole node, when `whole` is set (another day's preview, which carries
 *   no marks).
 *
 * The words stay where they are and only their ink goes transparent, so
 * veiling never reflows the line. One canvas per node draws a cloud of specks
 * over each hidden span. Every speck has two places: a loose home inside the
 * span's box, and a point on the span's own letterforms, sampled from the
 * glyphs as they are set on the page. Hovering a span (or lifting the veil)
 * pulls its specks onto the letters, and the real text fades in once they
 * have made the word; leaving lets them drift apart again. Under reduced
 * motion the specks sit still and the change is immediate.
 */

import type { Action } from "svelte/action";

export interface VeilParams {
	hiding: boolean;
	phrases?: string[];
	whole?: boolean;
}

interface Speck {
	hx: number;
	hy: number;
	gx: number;
	gy: number;
	phase: number;
	alpha: number;
}

interface Hidden {
	el: HTMLElement;
	/** The word's own color, which the reveal fades back to. */
	ink: string;
	/** 0 = scattered, 1 = gathered into the word. */
	t: number;
	target: number;
	specks: Speck[];
}

/** Letterform sampling pitch, in CSS pixels. */
const PITCH = 1.5;
/** Where the text starts to show through, as the specks arrive. */
const REVEAL_AT = 0.8;

function wrapPhrases(root: HTMLElement, phrases: string[]) {
	if (!phrases.length) return;
	const wanted = [...new Set(phrases.filter((p) => p.trim().length > 0))].sort((a, b) => b.length - a.length);
	const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
	const texts: Text[] = [];
	for (let n = walker.nextNode(); n; n = walker.nextNode()) {
		const parent = (n as Text).parentElement;
		if (parent?.closest("[data-veil], .ref-link")) continue;
		texts.push(n as Text);
	}
	for (const text of texts) {
		let node: Text | null = text;
		while (node) {
			const value: string = node.data;
			let hit: { at: number; len: number } | null = null;
			for (const p of wanted) {
				const at = value.indexOf(p);
				if (at >= 0 && (!hit || at < hit.at)) hit = { at, len: p.length };
			}
			if (!hit) break;
			const match: Text = node.splitText(hit.at);
			const rest: Text = match.splitText(hit.len);
			const span = document.createElement("span");
			span.dataset.veil = "phrase";
			match.replaceWith(span);
			span.appendChild(match);
			node = rest;
		}
	}
}

/** Hide everything: each run of text gets its own span, so links and other
 *  structure survive being hidden and shown again. */
function wrapAll(root: HTMLElement) {
	const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
	const texts: Text[] = [];
	for (let n = walker.nextNode(); n; n = walker.nextNode()) {
		const t = n as Text;
		if (!t.data.trim() || t.parentElement?.closest("[data-veil]")) continue;
		texts.push(t);
	}
	for (const t of texts) {
		const span = document.createElement("span");
		span.dataset.veil = "all";
		t.replaceWith(span);
		span.appendChild(t);
	}
}

function targets(root: HTMLElement): HTMLElement[] {
	const all = [...root.querySelectorAll<HTMLElement>(".ref-link, [data-veil]")];
	// A link that holds a wrapped run is drawn through that run, not twice.
	return all.filter((el) => !all.some((o) => o !== el && el.contains(o)));
}

function ease(t: number) {
	return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

/** Points on the span's letterforms, relative to `box`. */
function glyphPoints(el: HTMLElement, box: DOMRect, scratch: CanvasRenderingContext2D): { x: number; y: number }[] {
	const style = getComputedStyle(el);
	const rects = [...el.getClientRects()];
	if (!rects.length) return [];
	const left = Math.min(...rects.map((r) => r.left));
	const top = Math.min(...rects.map((r) => r.top));
	const w = Math.ceil(Math.max(...rects.map((r) => r.right)) - left) + 2;
	const h = Math.ceil(Math.max(...rects.map((r) => r.bottom)) - top) + 2;
	const canvas = scratch.canvas;
	if (canvas.width < w || canvas.height < h) {
		canvas.width = Math.max(canvas.width, w);
		canvas.height = Math.max(canvas.height, h);
	}
	scratch.clearRect(0, 0, canvas.width, canvas.height);
	scratch.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
	scratch.textBaseline = "alphabetic";
	scratch.fillStyle = "#000";
	const ascent = scratch.measureText("Hg").fontBoundingBoxAscent;

	// Each character where the page actually set it, line breaks included.
	const range = document.createRange();
	const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
	for (let n = walker.nextNode(); n; n = walker.nextNode()) {
		const text = n as Text;
		for (let i = 0; i < text.data.length; i++) {
			const ch = text.data[i];
			if (!ch.trim()) continue;
			range.setStart(text, i);
			range.setEnd(text, i + 1);
			const r = range.getBoundingClientRect();
			if (!r.width) continue;
			scratch.fillText(ch, r.left - left, r.top - top + ascent);
		}
	}

	const data = scratch.getImageData(0, 0, w, h).data;
	const out: { x: number; y: number }[] = [];
	for (let y = 0; y < h; y += PITCH) {
		for (let x = 0; x < w; x += PITCH) {
			if (data[(Math.floor(y) * w + Math.floor(x)) * 4 + 3] > 110) {
				out.push({ x: x + left - box.left, y: y + top - box.top });
			}
		}
	}
	return out;
}

function scatter(el: HTMLElement, box: DOMRect, n: number, rand: () => number): { x: number; y: number }[] {
	const rects = [...el.getClientRects()].filter((r) => r.width > 0);
	const area = rects.reduce((a, r) => a + r.width, 0) || 1;
	const out: { x: number; y: number }[] = [];
	for (let i = 0; i < n; i++) {
		let pick = rand() * area;
		let r = rects[0];
		for (const c of rects) {
			if (pick <= c.width) {
				r = c;
				break;
			}
			pick -= c.width;
		}
		out.push({
			x: r.left - box.left + rand() * r.width,
			y: r.top - box.top + r.height * (0.2 + rand() * 0.6),
		});
	}
	return out;
}

export const veiled: Action<HTMLElement, VeilParams> = (node, params) => {
	const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
	const scratch = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
	let hidden: Hidden[] = [];
	let canvas: HTMLCanvasElement | null = null;
	let raf = 0;
	let frame = 0;
	let hiding = false;
	let resize: ResizeObserver | null = null;

	function layout() {
		if (!scratch) return;
		const box = node.getBoundingClientRect();
		let seed = 7;
		const rand = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
		for (const h of hidden) {
			const glyphs = glyphPoints(h.el, box, scratch);
			const homes = scatter(h.el, box, glyphs.length, rand);
			h.specks = glyphs.map((g, i) => ({
				gx: g.x,
				gy: g.y,
				hx: homes[i].x,
				hy: homes[i].y,
				phase: rand() * Math.PI * 2,
				alpha: 0.3 + rand() * 0.5,
			}));
		}
	}

	function ensureCanvas() {
		if (canvas) return;
		canvas = document.createElement("canvas");
		canvas.setAttribute("aria-hidden", "true");
		Object.assign(canvas.style, {
			position: "absolute",
			inset: "0",
			pointerEvents: "none",
			width: "100%",
			height: "100%",
		});
		if (getComputedStyle(node).position === "static") node.style.position = "relative";
		node.appendChild(canvas);
		resize = new ResizeObserver(() => layout());
		resize.observe(node);
	}

	function teardown() {
		cancelAnimationFrame(raf);
		raf = 0;
		resize?.disconnect();
		resize = null;
		canvas?.remove();
		canvas = null;
		for (const h of hidden) {
			h.el.classList.remove("veil-hidden");
			h.el.style.removeProperty("color");
		}
		hidden = [];
	}

	function draw() {
		raf = 0;
		if (!canvas) return;
		const ctx = canvas.getContext("2d");
		if (!ctx) return;
		const dpr = window.devicePixelRatio || 1;
		const box = node.getBoundingClientRect();
		if (canvas.width !== Math.round(box.width * dpr) || canvas.height !== Math.round(box.height * dpr)) {
			canvas.width = Math.round(box.width * dpr);
			canvas.height = Math.round(box.height * dpr);
		}
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, box.width, box.height);
		ctx.fillStyle = getComputedStyle(node).color || "#1a2030";

		let moving = false;
		for (const h of hidden) {
			// About half a second to gather: slow enough to watch the word form.
			const step = still ? 1 : 0.035;
			if (h.t < h.target) h.t = Math.min(h.target, h.t + step);
			else if (h.t > h.target) h.t = Math.max(h.target, h.t - step * 0.7);
			if (h.t !== h.target) moving = true;
			const e = ease(h.t);
			// The text shows through as the specks arrive; the specks fade as it does.
			const show = Math.max(0, (h.t - REVEAL_AT) / (1 - REVEAL_AT));
			// Inline, so it outranks the stylesheet's transparent ink.
			if (show > 0) h.el.style.setProperty("color", `color-mix(in srgb, ${h.ink} ${(show * 100).toFixed(1)}%, transparent)`, "important");
			else h.el.style.removeProperty("color");
			const fade = 1 - show;
			if (fade <= 0) continue;
			for (const s of h.specks) {
				const drift = still ? 0 : (1 - e) * 1.2;
				const x = s.hx + (s.gx - s.hx) * e + Math.sin(frame / 18 + s.phase) * drift;
				const y = s.hy + (s.gy - s.hy) * e + Math.cos(frame / 23 + s.phase) * drift;
				ctx.globalAlpha = (s.alpha + (0.9 - s.alpha) * e) * fade;
				ctx.fillRect(x, y, 1.2, 1.2);
			}
		}
		frame++;

		if (!hiding && !moving) {
			teardown();
			return;
		}
		// Drifting specks keep the loop alive; a still veil draws once per change.
		if (!still || moving) raf = requestAnimationFrame(draw);
	}

	function kick() {
		if (!raf) raf = requestAnimationFrame(draw);
	}

	function onOver(e: PointerEvent) {
		if (!hiding) return;
		const el = (e.target as HTMLElement | null)?.closest?.(".veil-hidden");
		const h = hidden.find((x) => x.el === el);
		if (h) {
			h.target = 1;
			kick();
		}
	}

	function onOut(e: PointerEvent) {
		if (!hiding) return;
		const el = (e.target as HTMLElement | null)?.closest?.(".veil-hidden");
		const to = (e.relatedTarget as HTMLElement | null)?.closest?.(".veil-hidden");
		if (!el || el === to) return;
		const h = hidden.find((x) => x.el === el);
		if (h) {
			h.target = 0;
			kick();
		}
	}

	node.addEventListener("pointerover", onOver);
	node.addEventListener("pointerout", onOut);

	function apply(p: VeilParams, first: boolean) {
		hiding = p.hiding;
		if (p.hiding) {
			if (p.whole) wrapAll(node);
			else wrapPhrases(node, p.phrases ?? []);
			const els = targets(node);
			if (!els.length) return;
			const known = new Set(hidden.map((h) => h.el));
			for (const el of els) {
				if (known.has(el)) continue;
				const ink = getComputedStyle(el).color;
				el.classList.add("veil-hidden");
				// Arriving veiled, the words start hidden; turning the veil on, they dissolve.
				hidden.push({ el, ink, t: first ? 0 : 1, target: 0, specks: [] });
			}
			for (const h of hidden) h.target = 0;
			ensureCanvas();
			layout();
			kick();
		} else if (hidden.length) {
			for (const h of hidden) h.target = 1;
			kick();
		}
	}

	// After the markdown has rendered into the node, and its fonts have loaded
	// (the letterforms are sampled from the page).
	let first = true;
	queueMicrotask(() => {
		apply(params, first);
		first = false;
		document.fonts?.ready.then(() => hidden.length && layout());
	});

	return {
		update(p) {
			queueMicrotask(() => apply(p, false));
		},
		destroy() {
			node.removeEventListener("pointerover", onOver);
			node.removeEventListener("pointerout", onOut);
			teardown();
		},
	};
};
