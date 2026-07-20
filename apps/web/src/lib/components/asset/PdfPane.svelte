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

			container.replaceChildren(canvas, textDiv);
		} catch {
			renderedAt.delete(pageNum);
		}
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
				></div>
			{/each}
		{/if}
	</div>
</div>

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
