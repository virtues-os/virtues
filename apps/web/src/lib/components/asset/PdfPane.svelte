<script lang="ts">
	// PDF pane for AssetView, replacing the browser-plugin iframe. pdf.js gives
	// what the iframe can't: a selectable text layer, page addressing (the
	// `?page=N` contract citations will point at), and identical rendering in
	// every shell (Chromium, Safari, Tauri WKWebView).
	//
	// The library (~1MB gz with worker) is imported dynamically — the cost is
	// paid on first PDF open, never on app load. Pages render lazily via
	// IntersectionObserver; the download route's Range support (Phase 2) lets
	// pdf.js fetch large documents piecewise instead of whole.
	import Icon from "$lib/components/Icon.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import {
		listAnnotations,
		createAnnotation,
		updateAnnotation,
		deleteAnnotation,
		type Annotation,
		type AnnotationRect,
	} from "$lib/api/client";

	let {
		url,
		fileId,
		initialPage,
	}: {
		url: string;
		fileId: string;
		initialPage?: number;
	} = $props();

	type PdfjsModule = typeof import("pdfjs-dist");
	type PdfDocument = import("pdfjs-dist").PDFDocumentProxy;
	type PdfLoadingTask = import("pdfjs-dist").PDFDocumentLoadingTask;

	let pdfjs: PdfjsModule | null = null;
	let loadingTask: PdfLoadingTask | null = null;
	let doc = $state<PdfDocument | null>(null);
	let numPages = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// 1 = fit container width. Page aspect from page 1 sizes placeholders.
	let zoom = $state(1);
	let baseSize = $state<{ width: number; height: number }>({ width: 612, height: 792 });
	let currentPage = $state(1);
	let pageInput = $state("1");

	let scroller: HTMLDivElement | null = $state(null);
	let containerWidth = $state(0);
	// Bumped on zoom/resize; pages re-render when their rendered epoch is
	// stale. Deliberately NOT $state — it's only consulted inside async render
	// calls, and reading it reactively in the re-render effect would make the
	// `+= 1` self-triggering (infinite effect loop).
	let renderEpoch = 0;

	const lastPageKey = () => `pdf:last-page:${fileId}`;

	const scale = $derived(
		containerWidth > 0 ? ((containerWidth - 32) / baseSize.width) * zoom : zoom
	);
	const pageWidth = $derived(baseSize.width * scale);
	const pageHeight = $derived(baseSize.height * scale);

	$effect(() => {
		const target = url;
		loading = true;
		error = null;
		doc = null;
		numPages = 0;
		let cancelled = false;

		(async () => {
			try {
				const lib = await import("pdfjs-dist");
				const worker = await import("pdfjs-dist/build/pdf.worker.min.mjs?url");
				lib.GlobalWorkerOptions.workerSrc = worker.default;
				pdfjs = lib;

				const task = lib.getDocument({ url: target });
				loadingTask = task;
				const loaded = await task.promise;
				if (cancelled) {
					void task.destroy();
					return;
				}
				const first = await loaded.getPage(1);
				const vp = first.getViewport({ scale: 1 });
				baseSize = { width: vp.width, height: vp.height };
				numPages = loaded.numPages;
				doc = loaded;
				loading = false;

				// Deep link wins; otherwise restore the last-read page.
				const remembered = Number(localStorage.getItem(lastPageKey()) ?? NaN);
				const start = initialPage ?? (Number.isFinite(remembered) ? remembered : 1);
				if (start > 1) pendingJump = start;
			} catch (e) {
				if (!cancelled) {
					error = e instanceof Error ? e.message : "Failed to load PDF";
					loading = false;
				}
			}
		})();

		return () => {
			cancelled = true;
			void loadingTask?.destroy();
			loadingTask = null;
		};
	});

	// ── Per-page lazy rendering ─────────────────────────────────────────────
	const pageEls = new Map<number, HTMLDivElement>();
	const renderedAt = new Map<number, number>();
	let observer: IntersectionObserver | null = null;

	function registerPage(el: HTMLDivElement, pageNum: number) {
		pageEls.set(pageNum, el);
		observer?.observe(el);
		return {
			destroy() {
				observer?.unobserve(el);
				pageEls.delete(pageNum);
			},
		};
	}

	$effect(() => {
		if (!scroller || !doc) return;
		const io = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) {
					if (!entry.isIntersecting) continue;
					const n = Number((entry.target as HTMLElement).dataset.page);
					void renderPage(n);
				}
			},
			{ root: scroller, rootMargin: "800px 0px" }
		);
		observer = io;
		for (const el of pageEls.values()) io.observe(el);
		return () => {
			io.disconnect();
			observer = null;
		};
	});

	// Re-render already-rendered pages when scale changes.
	$effect(() => {
		void scale;
		renderEpoch += 1;
		for (const n of [...renderedAt.keys()]) void renderPage(n);
	});

	async function renderPage(pageNum: number) {
		const container = pageEls.get(pageNum);
		if (!doc || !pdfjs || !container || scale <= 0) return;
		const epoch = renderEpoch;
		if (renderedAt.get(pageNum) === epoch) return;
		renderedAt.set(pageNum, epoch);

		try {
			const page = await doc.getPage(pageNum);
			const viewport = page.getViewport({ scale });
			if (epoch !== renderEpoch) return;

			const canvas = document.createElement("canvas");
			const dpr = window.devicePixelRatio || 1;
			canvas.width = Math.floor(viewport.width * dpr);
			canvas.height = Math.floor(viewport.height * dpr);
			canvas.style.width = `${viewport.width}px`;
			canvas.style.height = `${viewport.height}px`;
			await page.render({
				canvas,
				viewport,
				transform: dpr !== 1 ? [dpr, 0, 0, dpr, 0, 0] : undefined,
			}).promise;
			if (epoch !== renderEpoch) return;

			const textDiv = document.createElement("div");
			textDiv.className = "pdf-text-layer";
			textDiv.style.setProperty("--scale-factor", String(viewport.scale));
			const textLayer = new pdfjs.TextLayer({
				textContentSource: page.streamTextContent(),
				container: textDiv,
				viewport,
			});
			await textLayer.render();
			if (epoch !== renderEpoch) return;

			// Inject canvas + text layer into the inner render target, leaving
			// the Svelte-managed highlight overlay (a sibling) untouched.
			const target = container.querySelector<HTMLElement>(".pdf-render") ?? container;
			target.replaceChildren(canvas, textDiv);
		} catch {
			renderedAt.delete(pageNum);
		}
	}

	// ── Annotations (highlights + margin notes) ──────────────────────────────
	// Highlights are stored as normalized page-space rects (0..1), so drawing
	// them as percentage-positioned overlays is zoom-independent — no coupling
	// to the canvas render epoch.
	const HL_COLORS: Record<string, string> = {
		yellow: "rgba(255, 214, 0, 0.38)",
		green: "rgba(64, 209, 120, 0.34)",
		blue: "rgba(80, 160, 255, 0.32)",
		pink: "rgba(255, 120, 190, 0.34)",
	};
	function colorCss(c: string): string {
		return HL_COLORS[c] ?? HL_COLORS.yellow;
	}

	let annotations = $state<Annotation[]>([]);
	const annosByPage = $derived.by(() => {
		const m = new Map<number, Annotation[]>();
		for (const a of annotations) {
			const p = a.page_num ?? 1;
			(m.get(p) ?? m.set(p, []).get(p)!).push(a);
		}
		return m;
	});

	$effect(() => {
		const fid = fileId;
		annotations = [];
		listAnnotations(fid)
			.then((a) => (annotations = a))
			.catch(() => {});
	});

	// Floating selection toolbar state.
	let sel = $state<{
		x: number;
		y: number;
		pageNum: number;
		quote: string;
		prefix: string;
		suffix: string;
		rects: AnnotationRect[];
	} | null>(null);

	// Open note popover for an existing highlight.
	let activeAnno = $state<Annotation | null>(null);
	let noteDraft = $state("");
	let notePos = $state<{ x: number; y: number }>({ x: 0, y: 0 });

	function clearSelectionUi() {
		sel = null;
	}

	// On mouse-up, capture a text selection inside a page's text layer as
	// normalized page-space rects + quote (+ context to disambiguate repeats).
	function handleMouseUp(e: MouseEvent) {
		const selection = window.getSelection();
		if (!selection || selection.isCollapsed || selection.rangeCount === 0) {
			return;
		}
		const quote = selection.toString().trim();
		if (quote.length < 2) return;

		const range = selection.getRangeAt(0);
		// Which page? Walk up from the selection anchor to a .pdf-page.
		let node: Node | null = range.startContainer;
		let pageEl: HTMLElement | null = null;
		while (node) {
			if (node instanceof HTMLElement && node.classList.contains("pdf-page")) {
				pageEl = node;
				break;
			}
			node = node.parentNode;
		}
		if (!pageEl) return;
		const pageNum = Number(pageEl.dataset.page);
		const pageBox = pageEl.getBoundingClientRect();
		if (pageBox.width === 0 || pageBox.height === 0) return;

		// Client rects → normalized page-space.
		const rects: AnnotationRect[] = [];
		for (const r of range.getClientRects()) {
			if (r.width < 1 || r.height < 1) continue;
			rects.push({
				x: (r.left - pageBox.left) / pageBox.width,
				y: (r.top - pageBox.top) / pageBox.height,
				w: r.width / pageBox.width,
				h: r.height / pageBox.height,
			});
		}
		if (rects.length === 0) return;

		// Prefix/suffix context from the page's text-layer string.
		const pageText = pageEl.querySelector(".pdf-text-layer")?.textContent ?? "";
		const idx = pageText.indexOf(quote);
		const prefix = idx > 0 ? pageText.slice(Math.max(0, idx - 30), idx) : "";
		const suffix =
			idx >= 0 ? pageText.slice(idx + quote.length, idx + quote.length + 30) : "";

		sel = {
			x: e.clientX,
			y: e.clientY,
			pageNum,
			quote,
			prefix,
			suffix,
			rects,
		};
	}

	async function commitHighlight(color: string) {
		if (!sel) return;
		const s = sel;
		sel = null;
		window.getSelection()?.removeAllRanges();
		try {
			const created = await createAnnotation({
				file_id: fileId,
				page_num: s.pageNum,
				quote_text: s.quote,
				quote_prefix: s.prefix,
				quote_suffix: s.suffix,
				rects: s.rects,
				color,
			});
			// Replace any existing (upsert) or append.
			annotations = [...annotations.filter((a) => a.id !== created.id), created];
		} catch {
			/* transient — highlight not saved */
		}
	}

	function openNote(a: Annotation, e: MouseEvent) {
		e.stopPropagation();
		activeAnno = a;
		noteDraft = a.note_md;
		notePos = { x: e.clientX, y: e.clientY };
	}

	async function saveNote() {
		if (!activeAnno) return;
		const id = activeAnno.id;
		try {
			const up = await updateAnnotation(id, { note_md: noteDraft });
			annotations = annotations.map((a) => (a.id === id ? up : a));
			activeAnno = up;
		} catch {
			/* keep draft */
		}
	}

	async function setColor(color: string) {
		if (!activeAnno) return;
		const id = activeAnno.id;
		const up = await updateAnnotation(id, { color }).catch(() => null);
		if (up) {
			annotations = annotations.map((a) => (a.id === id ? up : a));
			activeAnno = up;
		}
	}

	async function removeAnno() {
		if (!activeAnno) return;
		const id = activeAnno.id;
		activeAnno = null;
		await deleteAnnotation(id).catch(() => {});
		annotations = annotations.filter((a) => a.id !== id);
	}

	// ── Navigation ──────────────────────────────────────────────────────────
	function scrollToPage(n: number) {
		const clamped = Math.max(1, Math.min(numPages, n));
		pageEls.get(clamped)?.scrollIntoView({ block: "start" });
	}

	// Initial jump (deep link / remembered page). Deferred until the pane has
	// real layout: a tab can mount hidden (display:none keeps containerWidth
	// at 0 indefinitely), and page heights are only final once the fit-width
	// scale is measured — jumping earlier lands on the wrong page.
	let pendingJump = $state<number | null>(null);
	$effect(() => {
		if (pendingJump === null) return;
		// A hidden or pre-layout pane reports only its padding (32px) as
		// width — wait for a real measurement before consuming the jump, or
		// the pages still have zero height and the scroll lands nowhere.
		if (containerWidth > 100 && doc && numPages > 0) {
			const target = pendingJump;
			pendingJump = null;
			const attempt = (tries: number) => {
				if (pageEls.has(target) || tries <= 0) scrollToPage(target);
				else setTimeout(() => attempt(tries - 1), 50);
			};
			setTimeout(() => attempt(10), 0);
		}
	});

	function handleScroll() {
		if (!scroller || numPages === 0) return;
		const top = scroller.getBoundingClientRect().top;
		let best = 1;
		let bestDist = Infinity;
		for (const [n, el] of pageEls) {
			const dist = Math.abs(el.getBoundingClientRect().top - top);
			if (dist < bestDist) {
				bestDist = dist;
				best = n;
			}
		}
		if (best !== currentPage) {
			currentPage = best;
			pageInput = String(best);
			localStorage.setItem(lastPageKey(), String(best));
		}
	}

	function commitPageInput() {
		const n = Number(pageInput);
		if (Number.isFinite(n)) scrollToPage(n);
		else pageInput = String(currentPage);
	}

	const ZOOM_STEPS = [0.5, 0.67, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 2, 3];
	function zoomBy(dir: 1 | -1) {
		const i = ZOOM_STEPS.findIndex((z) => z >= zoom - 0.001);
		const next = ZOOM_STEPS[Math.max(0, Math.min(ZOOM_STEPS.length - 1, i + dir))];
		if (next) zoom = next;
	}
</script>

<div class="pdf-pane">
	<div class="pdf-toolbar">
		<button class="pdf-btn" onclick={() => scrollToPage(currentPage - 1)} title="Previous page">
			<Icon icon="ri:arrow-up-s-line" width="14" />
		</button>
		<input
			class="pdf-page-input"
			bind:value={pageInput}
			onchange={commitPageInput}
			onkeydown={(e) => e.key === "Enter" && commitPageInput()}
		/>
		<span class="pdf-page-total">/ {numPages || "–"}</span>
		<button class="pdf-btn" onclick={() => scrollToPage(currentPage + 1)} title="Next page">
			<Icon icon="ri:arrow-down-s-line" width="14" />
		</button>
		<div class="pdf-toolbar-spacer"></div>
		<button class="pdf-btn" onclick={() => zoomBy(-1)} title="Zoom out">
			<Icon icon="ri:zoom-out-line" width="14" />
		</button>
		<span class="pdf-zoom-label">{Math.round(zoom * 100)}%</span>
		<button class="pdf-btn" onclick={() => zoomBy(1)} title="Zoom in">
			<Icon icon="ri:zoom-in-line" width="14" />
		</button>
	</div>

	<div
		class="pdf-scroller"
		bind:this={scroller}
		bind:clientWidth={containerWidth}
		onscroll={handleScroll}
		onmouseup={handleMouseUp}
	>
		{#if loading}
			<div class="pdf-status"><Icon icon="ri:loader-4-line" width="22" class="spin" /></div>
		{:else if error}
			<div class="pdf-status error">
				<Icon icon="ri:error-warning-line" width="22" />
				<span>{error}</span>
			</div>
		{:else}
			{#each Array.from({ length: numPages }, (_, i) => i + 1) as pageNum (pageNum)}
				<div
					class="pdf-page"
					data-page={pageNum}
					style="width: {pageWidth}px; height: {pageHeight}px;"
					use:registerPage={pageNum}
				>
					<!-- canvas + text layer injected here by renderPage -->
					<div class="pdf-render"></div>
					<!-- Svelte-managed highlight overlay: normalized rects →
					     percentage boxes, so zoom needs no re-render. -->
					<div class="pdf-anno-layer">
						{#each annosByPage.get(pageNum) ?? [] as a (a.id)}
							{#each a.rects as r}
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
									class="pdf-hl"
									style="left:{r.x * 100}%; top:{r.y * 100}%; width:{r.w *
										100}%; height:{r.h * 100}%; background:{colorCss(a.color)};"
									title={a.note_md || 'Highlight'}
									onclick={(e) => openNote(a, e)}
								></div>
							{/each}
						{/each}
					</div>
				</div>
			{/each}
		{/if}
	</div>
</div>

<!-- Selection toolbar: pick a color to highlight. -->
{#if sel}
	<div class="pdf-sel-toolbar" style="left:{sel.x}px; top:{sel.y + 10}px;">
		{#each Object.keys(HL_COLORS) as c}
			<button
				class="pdf-sel-swatch"
				style="background:{colorCss(c)};"
				title="Highlight {c}"
				aria-label="Highlight {c}"
				onclick={() => commitHighlight(c)}
			></button>
		{/each}
	</div>
{/if}

<!-- Note popover for an existing highlight. -->
{#if activeAnno}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="pdf-note-scrim" onclick={() => (activeAnno = null)}></div>
	<div class="pdf-note" style="left:{notePos.x}px; top:{notePos.y + 10}px;">
		<div class="pdf-note-swatches">
			{#each Object.keys(HL_COLORS) as c}
				<button
					class="pdf-sel-swatch"
					class:active={activeAnno.color === c}
					style="background:{colorCss(c)};"
					aria-label="Set {c}"
					onclick={() => setColor(c)}
				></button>
			{/each}
			<div class="pdf-note-spacer"></div>
			<button class="pdf-note-del" title="Delete highlight" onclick={removeAnno}>
				<Icon icon="ri:delete-bin-line" width="13" />
			</button>
		</div>
		<textarea
			class="pdf-note-input"
			bind:value={noteDraft}
			placeholder="Add a note…"
			onblur={saveNote}
		></textarea>
		{#if activeAnno.note_md && activeAnno.note_md === noteDraft}
			<div class="pdf-note-preview"><Markdown content={activeAnno.note_md} /></div>
		{/if}
	</div>
{/if}

<style>
	.pdf-pane {
		display: flex;
		flex-direction: column;
		width: 100%;
		height: 100%;
		min-height: 0;
	}

	.pdf-toolbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px 14px;
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}
	.pdf-toolbar-spacer {
		flex: 1;
	}
	.pdf-btn {
		display: inline-flex;
		align-items: center;
		padding: 4px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.pdf-btn:hover {
		background: var(--ref-pill-bg);
		color: var(--color-foreground);
	}
	.pdf-page-input {
		width: 3.5em;
		padding: 2px 4px;
		text-align: center;
		font-size: 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
	}
	.pdf-page-total,
	.pdf-zoom-label {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}
	.pdf-zoom-label {
		min-width: 3em;
		text-align: center;
	}

	.pdf-scroller {
		flex: 1;
		min-height: 0;
		overflow: auto;
		padding: 16px;
		background: var(--color-surface-sunken, #1a1a1a);
	}

	.pdf-page {
		position: relative;
		margin: 0 auto 16px;
		background: white;
		box-shadow: 0 1px 4px rgba(0, 0, 0, 0.35);
	}
	.pdf-page :global(canvas) {
		position: absolute;
		inset: 0;
	}

	/* Highlight overlay — normalized-rect boxes, percentage-positioned so they
	   track the page at any zoom. Below the text layer (z:1) so selection still
	   works, but pointer-events on the boxes themselves to catch clicks. */
	.pdf-anno-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
		z-index: 2;
	}
	.pdf-hl {
		position: absolute;
		pointer-events: auto;
		cursor: pointer;
		border-radius: 1px;
		mix-blend-mode: multiply;
		transition: filter 0.1s;
	}
	.pdf-hl:hover {
		filter: brightness(0.92);
	}

	/* Selection color toolbar + note popover */
	.pdf-sel-toolbar {
		position: fixed;
		z-index: 50;
		display: flex;
		gap: 4px;
		padding: 5px 6px;
		border-radius: 8px;
		background: var(--color-surface-elevated, #222);
		border: 1px solid var(--color-border);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
	}
	.pdf-sel-swatch {
		width: 18px;
		height: 18px;
		border-radius: 50%;
		border: 1px solid rgba(0, 0, 0, 0.2);
		cursor: pointer;
		padding: 0;
	}
	.pdf-sel-swatch.active {
		outline: 2px solid var(--color-primary);
		outline-offset: 1px;
	}
	.pdf-note-scrim {
		position: fixed;
		inset: 0;
		z-index: 49;
	}
	.pdf-note {
		position: fixed;
		z-index: 50;
		width: 260px;
		padding: 8px;
		border-radius: 10px;
		background: var(--color-surface-elevated, #222);
		border: 1px solid var(--color-border);
		box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45);
	}
	.pdf-note-swatches {
		display: flex;
		align-items: center;
		gap: 4px;
		margin-bottom: 6px;
	}
	.pdf-note-spacer {
		flex: 1;
	}
	.pdf-note-del {
		display: inline-flex;
		padding: 3px;
		border: none;
		background: transparent;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		border-radius: 5px;
	}
	.pdf-note-del:hover {
		color: var(--color-danger, #e5484d);
		background: var(--ref-pill-bg);
	}
	.pdf-note-input {
		width: 100%;
		min-height: 54px;
		resize: vertical;
		padding: 6px 8px;
		font-size: 0.8125rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
	}
	.pdf-note-preview {
		margin-top: 6px;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	/* Minimal text layer (pdf.js positions spans via inline styles + the
	   --scale-factor var set on the layer container). */
	.pdf-page :global(.pdf-text-layer) {
		position: absolute;
		inset: 0;
		overflow: hidden;
		line-height: 1;
		text-size-adjust: none;
		forced-color-adjust: none;
		transform-origin: 0 0;
		z-index: 1;
	}
	.pdf-page :global(.pdf-text-layer span),
	.pdf-page :global(.pdf-text-layer br) {
		color: transparent;
		position: absolute;
		white-space: pre;
		cursor: text;
		transform-origin: 0% 0%;
	}
	.pdf-page :global(.pdf-text-layer ::selection),
	.pdf-page :global(.pdf-text-layer span::selection) {
		background: rgba(64, 128, 255, 0.3);
	}

	.pdf-status {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		height: 100%;
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
	}
	.pdf-status.error {
		color: var(--color-danger, #e5484d);
	}
	.pdf-status :global(.spin) {
		animation: pdf-pane-spin 0.8s linear infinite;
	}
	@keyframes pdf-pane-spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
