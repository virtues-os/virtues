/**
 * `use:veiled` — hide a passage's names and hard phrases behind drifting ink.
 *
 * What is hidden inside the node:
 * - every entity link the markdown renders (`.ref-link`: people, places);
 * - every phrase in `phrases` (the spans the writer marked with ⟦ ⟧);
 * - the whole node, when `whole` is set (another day's preview, which carries
 *   no marks).
 *
 * The hidden words are taken OUT of the page while veiled, not painted over: a
 * CSS blur leaves them selectable, findable with ⌘F and readable by a screen
 * reader. Each span's text is kept in memory and replaced with figure spaces
 * of roughly the same width, so lifting the veil does not reflow the
 * paragraph. One canvas per node draws the particles over the spans' boxes;
 * under reduced motion they are still.
 */

import type { Action } from "svelte/action";

export interface VeilParams {
	hiding: boolean;
	phrases?: string[];
	whole?: boolean;
}

const FIGURE_SPACE = " ";
const saved = new WeakMap<HTMLElement, string>();

function wrapPhrases(root: HTMLElement, phrases: string[]) {
	if (!phrases.length) return;
	const wanted = [...new Set(phrases.filter((p) => p.trim().length > 0))].sort((a, b) => b.length - a.length);
	const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
	const texts: Text[] = [];
	for (let n = walker.nextNode(); n; n = walker.nextNode()) {
		const parent = (n as Text).parentElement;
		if (parent?.closest("[data-veil]")) continue;
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
	return [...root.querySelectorAll<HTMLElement>(".ref-link, [data-veil]")];
}

function hide(el: HTMLElement) {
	if (saved.has(el)) return;
	const text = el.textContent ?? "";
	saved.set(el, text);
	el.textContent = FIGURE_SPACE.repeat(Math.max(1, Math.round(text.length * 0.9)));
	el.setAttribute("aria-label", "hidden");
	el.classList.add("veil-hidden");
}

function show(el: HTMLElement) {
	const text = saved.get(el);
	if (text === undefined) return;
	el.textContent = text;
	saved.delete(el);
	el.removeAttribute("aria-label");
	el.classList.remove("veil-hidden");
}

/** One canvas over the node, drawing specks inside the hidden spans' boxes. */
function particles(root: HTMLElement) {
	const canvas = document.createElement("canvas");
	canvas.setAttribute("aria-hidden", "true");
	Object.assign(canvas.style, {
		position: "absolute",
		inset: "0",
		pointerEvents: "none",
		width: "100%",
		height: "100%",
	});
	if (getComputedStyle(root).position === "static") root.style.position = "relative";
	root.appendChild(canvas);
	const ctx = canvas.getContext("2d");
	const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
	const ink = getComputedStyle(root).color || "#1a2030";
	let frame = 0;
	let raf = 0;

	function draw() {
		if (!ctx) return;
		const dpr = window.devicePixelRatio || 1;
		const box = root.getBoundingClientRect();
		if (canvas.width !== Math.round(box.width * dpr) || canvas.height !== Math.round(box.height * dpr)) {
			canvas.width = Math.round(box.width * dpr);
			canvas.height = Math.round(box.height * dpr);
		}
		ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		ctx.clearRect(0, 0, box.width, box.height);
		ctx.fillStyle = ink;
		let seed = 1;
		const rand = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
		for (const el of root.querySelectorAll<HTMLElement>(".veil-hidden")) {
			for (const r of el.getClientRects()) {
				const x0 = r.left - box.left;
				const y0 = r.top - box.top + r.height * 0.18;
				const h = r.height * 0.64;
				const n = Math.round((r.width * h) / 9);
				for (let i = 0; i < n; i++) {
					const phase = rand() * Math.PI * 2;
					const drift = still ? 0 : Math.sin(frame / 18 + phase) * 1.2;
					const x = x0 + rand() * r.width + drift;
					const y = y0 + rand() * h + (still ? 0 : Math.cos(frame / 23 + phase) * 0.9);
					ctx.globalAlpha = 0.25 + rand() * 0.55;
					ctx.fillRect(x, y, 1.1 + rand() * 0.6, 1.1 + rand() * 0.6);
				}
			}
		}
		frame++;
		if (!still) raf = requestAnimationFrame(draw);
	}
	draw();
	return () => {
		cancelAnimationFrame(raf);
		canvas.remove();
	};
}

export const veiled: Action<HTMLElement, VeilParams> = (node, params) => {
	let stop: (() => void) | null = null;

	function apply(p: VeilParams) {
		if (p.hiding) {
			if (p.whole) wrapAll(node);
			else wrapPhrases(node, p.phrases ?? []);
			const hidden = targets(node);
			for (const el of hidden) hide(el);
			// A passage with nothing to hide gets no canvas.
			if (!stop && hidden.length) stop = particles(node);
		} else {
			for (const el of node.querySelectorAll<HTMLElement>(".veil-hidden")) show(el);
			stop?.();
			stop = null;
		}
	}

	// After the markdown has rendered into the node.
	queueMicrotask(() => apply(params));

	return {
		update(p) {
			queueMicrotask(() => apply(p));
		},
		destroy() {
			stop?.();
		},
	};
};
