<script lang="ts">
	/**
	 * design.md as a page: its prose through the app's Markdown, its
	 * ```specimen``` blocks as live specimens, and after it renders:
	 *
	 * - every heading gets GitHub's slug as its id, so the file's own
	 *   `#section` links work here as they do on GitHub;
	 * - every inline token (`--chrome-row-h`) gets its current value beside
	 *   it, read from the cascade and read again when the theme changes. A
	 *   number the text claims sits next to the number the app draws, so a
	 *   stale line shows.
	 */
	import { tick } from 'svelte';
	import Markdown from '$lib/components/Markdown.svelte';
	import type { TocHeading } from '$lib/components/TableOfContents.svelte';
	import Specimen from './Specimen.svelte';
	import { slug, type DocPart } from '$lib/design/specimens';

	let { parts, onheadings }: { parts: DocPart[]; onheadings?: (h: TocHeading[]) => void } = $props();

	let root = $state<HTMLElement | null>(null);

	/** A whole token name. A prefix such as `--color-` names a family and gets no value. */
	const TOKEN = /^--[a-z0-9-]*[a-z0-9]$/;

	function decorate(el: HTMLElement) {
		const headings: TocHeading[] = [];
		const seen = new Map<string, number>();
		for (const h of el.querySelectorAll<HTMLElement>('h1, h2, h3')) {
			let id = slug(h.textContent ?? '');
			const n = seen.get(id) ?? 0;
			seen.set(id, n + 1);
			if (n) id = `${id}-${n}`;
			if (h.id !== id) h.id = id;
			if (h.tagName !== 'H1') headings.push({ id, text: h.textContent ?? '', level: h.tagName === 'H2' ? 2 : 3 });
		}
		onheadings?.(headings);

		const cs = getComputedStyle(document.documentElement);
		for (const code of el.querySelectorAll<HTMLElement>('code')) {
			if (code.closest('pre, .specimen')) continue;
			const name = code.textContent?.trim() ?? '';
			if (!TOKEN.test(name)) continue;
			const value = cs.getPropertyValue(name).trim() || 'not defined';
			let chip = code.nextElementSibling as HTMLElement | null;
			if (!chip?.classList.contains('token-value')) {
				chip = document.createElement('span');
				chip.className = 'token-value';
				code.after(chip);
			}
			if (chip.textContent !== value) chip.textContent = value;
			chip.classList.toggle('missing', value === 'not defined');
		}
	}

	$effect(() => {
		const el = root;
		if (!el) return;
		// A timer, not a frame: a background tab runs no animation frames, and
		// the page should be decorated when you come back to it.
		let frame: ReturnType<typeof setTimeout> | undefined;
		const schedule = () => {
			clearTimeout(frame);
			frame = setTimeout(() => decorate(el), 30);
		};
		// Markdown renders after mount, and a specimen can arrive later still.
		void tick().then(schedule);
		const observer = new MutationObserver(schedule);
		observer.observe(el, { childList: true, subtree: true });
		window.addEventListener('themechange', schedule);
		return () => {
			clearTimeout(frame);
			observer.disconnect();
			window.removeEventListener('themechange', schedule);
		};
	});

	/** The file's own `#section` links scroll the page rather than navigate. */
	function onclick(e: MouseEvent) {
		const a = (e.target as HTMLElement).closest('a');
		const href = a?.getAttribute('href') ?? '';
		if (!href.startsWith('#')) return;
		const target = root?.querySelector(`[id="${CSS.escape(decodeURIComponent(href.slice(1)))}"]`);
		if (!target) return;
		e.preventDefault();
		e.stopPropagation();
		target.scrollIntoView({ behavior: 'smooth', block: 'start' });
	}
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="design-doc" bind:this={root} onclickcapture={onclick}>
	{#each parts as part, i (i)}
		{#if part.kind === 'markdown'}
			<Markdown content={part.text} />
		{:else}
			<Specimen name={part.name} />
		{/if}
	{/each}
</div>

<style>
	.design-doc {
		padding-bottom: 64px;
	}

	.design-doc :global(h1) {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-title);
		line-height: 1.15;
		margin: 0 0 24px;
	}

	.design-doc :global(h2) {
		scroll-margin-top: 24px;
	}

	.design-doc :global(h3) {
		scroll-margin-top: 24px;
	}

	.design-doc :global(.token-value) {
		margin-left: 4px;
		padding: 0 4px;
		border-radius: var(--radius-sm);
		background: var(--fill-bg);
		font-family: var(--font-sans);
		font-size: var(--text-caption);
		color: var(--color-foreground-muted);
		white-space: nowrap;
		font-variant-numeric: tabular-nums;
	}

	.design-doc :global(.token-value.missing) {
		color: var(--color-error);
	}
</style>
