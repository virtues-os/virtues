<!--
	TableOfContents.svelte

	Floating minimap-style ToC: vertical stack of horizontal lines
	representing h2/h3 sections. On hover, individual lines expand
	and reveal their label. Sticky-positioned in the right gutter.
-->

<script lang="ts">
	import { browser } from "$app/environment";

	export interface TocHeading {
		id: string;
		text: string;
		level: 2 | 3;
	}

	interface Props {
		headings: TocHeading[];
		/** The scrollable container to observe for scroll-spy */
		scrollContainer?: HTMLElement | null;
	}

	let { headings, scrollContainer = null }: Props = $props();

	let activeId = $state<string | null>(null);
	let hoveredIndex = $state<number | null>(null);

	// Scroll-spy: track which section is currently in view
	$effect(() => {
		if (!browser || !scrollContainer || headings.length === 0) return;

		const observer = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) {
					if (entry.isIntersecting) {
						activeId = entry.target.id;
					}
				}
			},
			{
				root: scrollContainer,
				rootMargin: "-10% 0px -80% 0px",
				threshold: 0,
			},
		);

		for (const h of headings) {
			const el = scrollContainer.querySelector(`#${CSS.escape(h.id)}`);
			if (el) observer.observe(el);
		}

		return () => observer.disconnect();
	});

	function scrollToSection(id: string) {
		if (!scrollContainer) return;
		const el = scrollContainer.querySelector(`#${CSS.escape(id)}`);
		if (el) {
			el.scrollIntoView({ behavior: "smooth", block: "start" });
		}
	}
</script>

{#if headings.length > 0}
	<nav
		class="toc"
		aria-label="Table of contents"
		onmouseleave={() => { hoveredIndex = null; }}
	>
		<div class="toc-lines">
			{#each headings as heading, i}
				<button
					type="button"
					class="toc-row"
					class:active={activeId === heading.id}
					class:hovered={hoveredIndex === i}
					class:is-h3={heading.level === 3}
					onmouseenter={() => { hoveredIndex = i; }}
					onmouseleave={() => { hoveredIndex = null; }}
					onclick={() => scrollToSection(heading.id)}
				>
					<div class="toc-line"></div>
					<span class="toc-label">
						{heading.text}
					</span>
				</button>
			{/each}
		</div>
	</nav>
{/if}

<style>
	.toc {
		position: sticky;
		top: 3rem;
		align-self: flex-start;
		z-index: 10;
		display: flex;
		flex-direction: column;
		padding: 0.5rem 0;
		flex-shrink: 0;
	}

	/* Hide when viewport is too narrow for the gutter */
	@media (max-width: 1200px) {
		.toc {
			display: none;
		}
	}

	.toc-lines {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.toc-row {
		all: unset;
		display: flex;
		align-items: center;
		gap: 10px;
		cursor: pointer;
		padding: 1px 0;
		position: relative;
	}

	.toc-line {
		height: 1.5px;
		border-radius: 1px;
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
		transform-origin: left center;
		transition:
			background 0.15s ease,
			transform 0.2s cubic-bezier(0.22, 1, 0.36, 1),
			width 0.2s cubic-bezier(0.22, 1, 0.36, 1);
		width: 18px;
	}

	.toc-row.is-h3 .toc-line {
		width: 10px;
		margin-left: 4px;
	}

	/* Hovered state: line stretches */
	.toc-row.hovered .toc-line {
		transform: scaleX(1.4);
		background: color-mix(in srgb, var(--color-foreground) 45%, transparent);
	}

	/* Active state */
	.toc-row.active .toc-line {
		background: var(--color-foreground);
		height: 2px;
	}
	.toc-row.active.hovered .toc-line {
		transform: scaleX(1.4);
		background: var(--color-foreground);
	}

	/* Label: hidden by default, slides in on hover */
	.toc-label {
		font-family: var(--font-sans, system-ui, sans-serif);
		font-size: 0.6875rem;
		font-weight: 450;
		color: var(--color-foreground-muted);
		white-space: nowrap;
		letter-spacing: 0.01em;
		user-select: none;
		opacity: 0;
		transform: translateX(-6px);
		transition:
			opacity 0.18s ease-out,
			transform 0.18s cubic-bezier(0.22, 1, 0.36, 1);
		pointer-events: none;
	}

	.toc-row.hovered .toc-label {
		opacity: 1;
		transform: translateX(0);
	}

	.toc-row.active .toc-label {
		color: var(--color-foreground);
		font-weight: 500;
	}
</style>
