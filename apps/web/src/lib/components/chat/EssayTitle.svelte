<!--
	EssayTitle.svelte

	The chat's name at the top of the essay layout. A name that is not
	settled yet is drawn as particles: its letters are sampled into dots on
	a small grid, and the dots stand in for the type until a real name
	arrives.

	  carry     on the first send, home's headline flies here (720 ms),
	            already in dots
	  drafted   waiting for a name: each dot drifts a fraction of a pixel
	            and its brightness varies, and a slow wave crosses the
	            words every 3.2 s, swelling the dots it passes
	  morph     a real name arrived: the same dots travel from the old
	            letters to the new ones, the left side first, along slight
	            arcs (about 1.3 s); spare dots fade on the way
	  resolve   the type fades in under the dots as they fade out (320 ms)
	  settled   plain type, no canvas

	The name never repeats your first message. A generated title that does
	is passed over, and the day-part name ("Friday afternoon") settles in
	its place. A settled name is plain contenteditable: click anywhere in
	it to type, Enter or leaving it keeps the name, Escape puts it back.
	With Reduce motion the dots hold still and the words swap.
-->
<script lang="ts" module>
	/** Where home's headline was when you sent, for the carry. */
	export interface CarryFrom {
		left: number;
		top: number;
		fontSize: number;
	}
</script>

<script lang="ts">
	import { onMount, tick } from "svelte";
	import { dayPartHeadline } from "$lib/utils/dateUtils";
	import { reducedMotion } from "$lib/stores/motion.svelte";

	interface Props {
		/** The stored title, "" until one is written. */
		title: string;
		/** Your first message, which the name must not repeat. */
		firstMessage: string;
		/** When the chat started; now for a chat started this moment. */
		startedAt?: string;
		/** Home's headline, when this chat was started from it just now. */
		from?: CarryFrom | null;
		/** The carry is over, so home's headline can go. */
		oncarried?: () => void;
		/** Save a name typed into the title. */
		onrename?: (title: string) => void;
	}

	let { title, firstMessage, startedAt, from = null, oncarried, onrename }: Props = $props();

	const CARRY_MS = 720;
	const MOVE_MS = 950;
	const STAGGER_MS = 380;
	const RESOLVE_MS = 320;
	const WAVE_MS = 3200;
	// A chat this new is still waiting on its name. An older one with none
	// never got one, and its day-part name is final, not provisional.
	const FRESH_MS = 3 * 60_000;
	// Sampling grid in CSS px, the most dots a name may use, and their size.
	const STEP = 3;
	const MAX_DOTS = 1800;
	const DOT_R = 1.05;

	const words = (s: string) => s.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, " ").trim();
	function repeats(t: string, message: string): boolean {
		const a = words(t);
		const b = words(message);
		return a !== "" && (a === b || b.startsWith(a));
	}

	// A name you typed, shown as typed even if it repeats your message.
	let typed = $state<string | null>(null);
	const started = $derived(startedAt ? new Date(startedAt) : new Date());
	const real = $derived(title && !repeats(title, firstMessage) ? title : "");
	const target = $derived(typed ?? (real || dayPartHeadline(started)));
	// A title that only repeats your message is the placeholder the chat
	// starts with, so it still counts as waiting. Past FRESH_MS no real name
	// is coming and the day-part name settles.
	let expired = $state(false);
	const waiting = $derived(
		typed === null && !real && !expired && (from !== null || Date.now() - started.getTime() < FRESH_MS),
	);

	type Phase = "drafted" | "morph" | "resolve" | "settled";
	let shown = $state("");
	let phase = $state<Phase>("settled");
	let ready = $state(false);
	let editing = $state(false);
	let el: HTMLHeadingElement | undefined = $state();
	let wordsEl: HTMLSpanElement | undefined = $state();
	let canvas: HTMLCanvasElement | undefined = $state();

	// ── Particles ──

	interface Dot {
		x: number; y: number; // where this move starts
		tx: number; ty: number; // where it ends
		a: number; ta: number; // alpha at the start and at the end
		delay: number;
		lift: number; // the move's arc height
		seed: number;
	}
	let dots: Dot[] = [];
	let moveStart = 0;
	let resolveStart = 0;
	let frame = 0;
	let cssW = 0;
	let cssH = 0;
	let ink = "#111";

	/** The letters of `text` as points, laid out the way the h1 sets them. */
	function sample(text: string): { x: number; y: number }[] {
		if (!el) return [];
		const cs = getComputedStyle(el);
		const width = el.clientWidth;
		const lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.2;
		const font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
		const dpr = Math.min(window.devicePixelRatio || 1, 2);
		const off = document.createElement("canvas");
		const ctx = off.getContext("2d", { willReadFrequently: true });
		if (!ctx) return [];
		const spaced = ctx as unknown as { letterSpacing?: string };
		ctx.font = font;
		if ("letterSpacing" in ctx) spaced.letterSpacing = cs.letterSpacing;
		// Wrap on words to the h1's width, as the browser does.
		const lines: string[] = [];
		let line = "";
		for (const w of text.split(/\s+/)) {
			const next = line ? `${line} ${w}` : w;
			if (line && ctx.measureText(next).width > width) {
				lines.push(line);
				line = w;
			} else line = next;
		}
		if (line) lines.push(line);
		const m = ctx.measureText("Hg");
		const asc = m.fontBoundingBoxAscent ?? parseFloat(cs.fontSize) * 0.8;
		const desc = m.fontBoundingBoxDescent ?? parseFloat(cs.fontSize) * 0.2;
		const baseline = (lh - (asc + desc)) / 2 + asc;
		const h = Math.ceil(lines.length * lh);
		off.width = Math.max(1, Math.ceil(width * dpr));
		off.height = Math.max(1, Math.ceil(h * dpr));
		// Resizing a canvas resets its state.
		ctx.scale(dpr, dpr);
		ctx.font = font;
		if ("letterSpacing" in ctx) spaced.letterSpacing = cs.letterSpacing;
		ctx.fillStyle = "#000";
		lines.forEach((l, i) => ctx.fillText(l, 0, i * lh + baseline));
		const data = ctx.getImageData(0, 0, off.width, off.height).data;
		let step = STEP;
		let pts: { x: number; y: number }[] = [];
		for (;;) {
			pts = [];
			for (let y = step / 2; y < h; y += step) {
				for (let x = step / 2; x < width; x += step) {
					const i = (Math.floor(y * dpr) * off.width + Math.floor(x * dpr)) * 4 + 3;
					if (data[i] > 110) pts.push({ x: x + (Math.random() - 0.5) * 0.9, y: y + (Math.random() - 0.5) * 0.9 });
				}
			}
			if (pts.length <= MAX_DOTS) break;
			step += 0.5;
		}
		return pts;
	}

	function sizeCanvas(maxY: number) {
		if (!el || !canvas) return;
		cssW = el.clientWidth;
		cssH = Math.max(el.clientHeight, maxY + 8);
		const dpr = Math.min(window.devicePixelRatio || 1, 2);
		canvas.width = Math.ceil(cssW * dpr);
		canvas.height = Math.ceil(cssH * dpr);
		canvas.style.width = `${cssW}px`;
		canvas.style.height = `${cssH}px`;
		canvas.getContext("2d")?.setTransform(dpr, 0, 0, dpr, 0, 0);
		ink = getComputedStyle(el).color;
	}

	const maxY = (ps: { y: number; ty?: number }[]) => ps.reduce((m, p) => Math.max(m, p.y, p.ty ?? 0), 0);

	/** Dots standing on `text`'s letters, at rest. */
	function settleDots(text: string) {
		const pts = sample(text);
		dots = pts.map((p) => ({ x: p.x, y: p.y, tx: p.x, ty: p.y, a: 1, ta: 1, delay: 0, lift: 0, seed: Math.random() * 1000 }));
		sizeCanvas(maxY(pts));
	}

	/** Send the dots from where they are to `text`'s letters. */
	function morphTo(text: string) {
		const now = performance.now();
		// Fold a move in progress into the dots' current spots.
		for (const d of dots) {
			const p = positionAt(d, now);
			d.x = p.x;
			d.y = p.y;
			d.a = p.a;
		}
		const pts = sample(text).sort((a, b) => a.x - b.x || a.y - b.y);
		const old = [...dots].sort((a, b) => a.x - b.x || a.y - b.y);
		const n = Math.max(pts.length, old.length);
		const next: Dot[] = [];
		for (let i = 0; i < n; i++) {
			// Pair by rank along the line, so the dots travel in step rather
			// than crossing the whole title to reach a letter.
			const src = old.length ? old[Math.min(old.length - 1, Math.floor((i / n) * old.length))] : null;
			const dst = pts.length ? pts[Math.min(pts.length - 1, Math.floor((i / n) * pts.length))] : null;
			if (!src && !dst) continue;
			const sx = src?.x ?? dst!.x;
			const sy = src?.y ?? dst!.y;
			const tx = dst?.x ?? sx;
			const ty = dst?.y ?? sy;
			// When counts differ, every few dots is a newcomer (fades in) or a
			// spare (fades out), spread along the line.
			const newcomer = old.length < pts.length && (i * old.length) % n >= old.length;
			const spare = pts.length < old.length && (i * pts.length) % n >= pts.length;
			next.push({
				x: sx,
				y: sy,
				tx,
				ty,
				a: newcomer ? 0 : (src?.a ?? 1),
				ta: spare ? 0 : 1,
				delay: (tx / Math.max(cssW, 1)) * STAGGER_MS + Math.random() * 90,
				lift: (Math.random() - 0.5) * 14,
				seed: Math.random() * 1000,
			});
		}
		dots = next;
		sizeCanvas(maxY(next));
		moveStart = now;
	}

	const ease = (k: number) => (k < 0.5 ? 4 * k * k * k : 1 - Math.pow(-2 * k + 2, 3) / 2);

	function positionAt(d: Dot, t: number) {
		const k = phase === "morph" ? Math.min(1, Math.max(0, (t - moveStart - d.delay) / MOVE_MS)) : 1;
		const e = ease(k);
		return {
			x: d.x + (d.tx - d.x) * e,
			y: d.y + (d.ty - d.y) * e - Math.sin(Math.PI * e) * d.lift,
			a: d.a + (d.ta - d.a) * e,
		};
	}

	function draw(t: number) {
		const ctx = canvas?.getContext("2d");
		if (!ctx) return;
		ctx.clearRect(0, 0, cssW, cssH);
		ctx.fillStyle = ink;
		const still = reducedMotion();
		const waveX = ((t % WAVE_MS) / WAVE_MS) * (cssW + 240) - 120;
		const fade = phase === "resolve" ? Math.max(0, 1 - (t - resolveStart) / RESOLVE_MS) : 1;
		for (const d of dots) {
			const p = positionAt(d, t);
			let { x, y, a } = p;
			let r = DOT_R;
			if (!still) {
				// Drift and twinkle, each dot on its own slow phase.
				x += Math.sin(t * 0.0011 + d.seed) * 0.45;
				y += Math.cos(t * 0.0009 + d.seed * 1.7) * 0.45;
				a *= 0.62 + 0.38 * Math.sin(t * 0.0023 + d.seed * 3.1) ** 2;
				if (phase === "drafted") {
					const w = Math.exp(-(((x - waveX) / 46) ** 2));
					r += 0.75 * w;
					a = Math.min(1, a + 0.35 * w);
				}
			}
			if (phase === "resolve") {
				r += (1 - fade) * 0.6;
				a *= fade;
			}
			if (a <= 0.01) continue;
			ctx.globalAlpha = a * 0.85;
			ctx.beginPath();
			ctx.arc(x, y, r, 0, Math.PI * 2);
			ctx.fill();
		}
		ctx.globalAlpha = 1;
	}

	function loop(t: number) {
		if (phase === "settled" || !canvas) {
			frame = 0;
			return;
		}
		if (phase === "morph" && t - moveStart > MOVE_MS + STAGGER_MS + 120) {
			if (waiting) {
				// The provisional name changed while still provisional: stay in dots.
				dots = dots
					.map((d) => ({ ...d, x: d.tx, y: d.ty, a: d.ta }))
					.filter((d) => d.a > 0);
				phase = "drafted";
			} else {
				phase = "resolve";
				resolveStart = t;
			}
		}
		if (phase === "resolve" && t - resolveStart > RESOLVE_MS) {
			phase = "settled";
			dots = [];
			frame = 0;
			return;
		}
		draw(t);
		frame = requestAnimationFrame(loop);
	}

	function run() {
		if (!frame) frame = requestAnimationFrame(loop);
	}

	// ── Phases ──

	$effect(() => {
		const next = target;
		const resting = waiting;
		if (!ready || editing) return;
		if (next === shown) {
			// Same words, now final (the title repeated your message, or time
			// ran out): resolve in place.
			if (!resting && phase === "drafted") {
				phase = "resolve";
				resolveStart = performance.now();
				run();
			}
			return;
		}
		const was = phase;
		shown = next;
		if (was === "settled" || reducedMotion()) {
			// A settled name renamed elsewhere, or no motion: the words swap.
			if (was !== "settled" && !resting) {
				phase = "settled";
				dots = [];
			} else if (was !== "settled") {
				void tick().then(() => settleDots(next));
			}
			return;
		}
		morphTo(next);
		phase = "morph";
		run();
	});

	// The words are set by hand, not by Svelte, because typing into a
	// contenteditable replaces the text node Svelte would otherwise update.
	$effect(() => {
		const text = shown;
		if (wordsEl && !editing && wordsEl.textContent !== text) wordsEl.textContent = text;
	});

	onMount(() => {
		const left = from ? FRESH_MS : FRESH_MS - (Date.now() - started.getTime());
		const expiry = setTimeout(() => (expired = true), Math.max(left, 0));
		let flight: Animation | null = null;
		let cancelled = false;
		const ro = new ResizeObserver(() => {
			if (phase === "drafted" && el && el.clientWidth !== cssW) settleDots(shown);
		});

		shown = target;
		phase = waiting ? "drafted" : "settled";
		void (async () => {
			await document.fonts.ready;
			await tick();
			if (cancelled) return;
			if (phase === "drafted") {
				settleDots(shown);
				run();
			}
			ready = true;
			if (el) ro.observe(el);
		})();

		if (from && el && !reducedMotion()) {
			const me = el.getBoundingClientRect();
			const scale = from.fontSize / parseFloat(getComputedStyle(el).fontSize);
			flight = el.animate(
				[{ transform: `translate(${from.left - me.left}px, ${from.top - me.top}px) scale(${scale})` }, { transform: "none" }],
				{ duration: CARRY_MS, easing: "cubic-bezier(0.2, 0, 0, 1)" },
			);
			flight.onfinish = () => oncarried?.();
			flight.oncancel = () => oncarried?.();
		} else {
			oncarried?.();
		}

		return () => {
			cancelled = true;
			clearTimeout(expiry);
			flight?.cancel();
			ro.disconnect();
			if (frame) cancelAnimationFrame(frame);
			frame = 0;
		};
	});

	// ── Renaming in place ──

	/** A click on dots or moving words settles them, then puts the caret there. */
	function settleForEdit(e: MouseEvent) {
		if (!onrename || phase === "settled") return;
		phase = "settled";
		dots = [];
		expired = true;
		const x = e.clientX;
		const y = e.clientY;
		void tick().then(() => {
			wordsEl?.focus();
			placeCaret(x, y);
		});
	}

	function placeCaret(x: number, y: number) {
		const doc = document as Document & {
			caretPositionFromPoint?: (x: number, y: number) => { offsetNode: Node; offset: number } | null;
			caretRangeFromPoint?: (x: number, y: number) => Range | null;
		};
		const range = document.createRange();
		const pos = doc.caretPositionFromPoint?.(x, y);
		if (pos) range.setStart(pos.offsetNode, pos.offset);
		else {
			const r = doc.caretRangeFromPoint?.(x, y);
			if (!r) return;
			range.setStart(r.startContainer, r.startOffset);
		}
		range.collapse(true);
		const sel = window.getSelection();
		sel?.removeAllRanges();
		sel?.addRange(range);
	}

	function onBlur() {
		if (!editing) return;
		editing = false;
		const next = (wordsEl?.textContent ?? "").replace(/\s+/g, " ").trim();
		if (!next) {
			if (wordsEl) wordsEl.textContent = shown;
			return;
		}
		if (next !== shown) {
			typed = next;
			shown = next;
			onrename?.(next);
		}
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === "Enter") {
			e.preventDefault();
			wordsEl?.blur();
		} else if (e.key === "Escape") {
			e.preventDefault();
			editing = false;
			if (wordsEl) wordsEl.textContent = shown;
			wordsEl?.blur();
		}
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<h1 bind:this={el} class="essay-title" aria-busy={phase !== "settled"} onclick={settleForEdit}>
	<span
		bind:this={wordsEl}
		class="words"
		class:veiled={phase === "drafted" || phase === "morph"}
		contenteditable={onrename && phase === "settled" ? "plaintext-only" : "false"}
		spellcheck="false"
		role="textbox"
		tabindex={onrename && phase === "settled" ? 0 : -1}
		aria-label="Chat name"
		onfocus={() => (editing = true)}
		onblur={onBlur}
		onkeydown={onKeydown}
	></span>
	{#if phase !== "settled"}
		<canvas bind:this={canvas} class="dots" aria-hidden="true"></canvas>
	{/if}
</h1>

<style>
	/* An h1, since it is the one title on the page, sized for a chat: a few
	   words over a column of 18px text, one clear step above a heading
	   inside a reply (--md-h1-size, 26px). */
	.essay-title {
		position: relative;
		margin: 0;
		font-family: var(--font-serif);
		font-size: 1.875rem;
		font-weight: 400;
		line-height: 1.2;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		transform-origin: left top;
	}

	/* Plain contenteditable: no box, no ring, the caret where you click. */
	.words {
		outline: none;
		cursor: text;
		transition: opacity 320ms var(--ease-premium);
	}
	/* Not `.hidden`: Tailwind's utility of that name is display: none. */
	.words.veiled {
		opacity: 0;
	}

	.dots {
		position: absolute;
		left: 0;
		top: 0;
		pointer-events: none;
	}
</style>
