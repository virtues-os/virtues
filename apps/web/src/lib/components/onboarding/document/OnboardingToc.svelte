<!--
  OnboardingToc — the left-margin table of contents that doubles as progress.

  A labeled rail (unlike the hover-minimap TableOfContents): the four chapters
  are always visible. Each row fills in as its chapter completes — pending
  (faint), active (ink, in view), done (filled rule + ink label). Scroll-spy
  via IntersectionObserver on the document's scroll container; click to jump.
  Hidden below 1200px (no gutter to live in).
-->
<script lang="ts">
	import { browser } from "$app/environment";

	export interface TocChapter {
		id: string;
		label: string;
	}

	interface Props {
		chapters: TocChapter[];
		completedIds?: string[];
		scrollContainer?: HTMLElement | null;
		reduced?: boolean;
	}

	let { chapters, completedIds = [], scrollContainer = null, reduced = false }: Props = $props();

	let activeId = $state<string | null>(null);

	$effect(() => {
		if (!browser || !scrollContainer || chapters.length === 0) return;
		const observer = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) {
					if (entry.isIntersecting) activeId = entry.target.id;
				}
			},
			{ root: scrollContainer, rootMargin: "-15% 0px -75% 0px", threshold: 0 },
		);
		for (const c of chapters) {
			const el = scrollContainer.querySelector(`#${CSS.escape(c.id)}`);
			if (el) observer.observe(el);
		}
		return () => observer.disconnect();
	});

	function jump(id: string) {
		if (!scrollContainer) return;
		const el = scrollContainer.querySelector(`#${CSS.escape(id)}`);
		if (el) el.scrollIntoView({ behavior: reduced ? "auto" : "smooth", block: "start" });
	}
</script>

<nav class="toc" aria-label="Onboarding progress">
	{#each chapters as chapter, i (chapter.id)}
		{@const done = completedIds.includes(chapter.id)}
		{@const active = activeId === chapter.id}
		<button
			type="button"
			class="toc-row"
			class:done
			class:active
			onclick={() => jump(chapter.id)}
		>
			<span class="toc-rule"></span>
			<span class="toc-label">{chapter.label}</span>
		</button>
	{/each}
</nav>

<style>
	@reference "../../../../app.css";

	.toc {
		position: sticky;
		top: 40vh;
		align-self: flex-start;
		display: flex;
		flex-direction: column;
		gap: 0.85rem;
		flex-shrink: 0;
		width: 9rem;
	}

	@media (max-width: 1200px) {
		.toc {
			display: none;
		}
	}

	.toc-row {
		all: unset;
		display: flex;
		align-items: center;
		gap: 0.7rem;
		cursor: pointer;
	}

	.toc-rule {
		height: 1.5px;
		width: 14px;
		border-radius: 1px;
		background: color-mix(in srgb, var(--color-foreground) 14%, transparent);
		transition:
			width 0.4s var(--ease-out-expo, cubic-bezier(0.16, 1, 0.3, 1)),
			background 0.3s ease;
		flex-shrink: 0;
	}

	.toc-label {
		font-family: var(--font-mono);
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
		transition: color 0.3s ease;
	}

	.toc-row.active .toc-rule {
		width: 22px;
		background: var(--color-primary);
	}
	.toc-row.active .toc-label {
		color: var(--color-primary);
	}

	.toc-row.done .toc-rule {
		width: 22px;
		background: color-mix(in srgb, var(--color-foreground) 55%, transparent);
	}
	.toc-row.done .toc-label {
		color: var(--color-foreground-muted);
	}
</style>
