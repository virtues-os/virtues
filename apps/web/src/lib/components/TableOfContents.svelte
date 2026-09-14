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
		level?: 2 | 3;
		/**
		 * What the row is, when the rows are steps rather than headings:
		 * `done` reads stronger than an untouched line, `skipped` is dashed
		 * (set aside is not the same as handled). Omitted for prose.
		 */
		state?: "done" | "skipped";
	}

	interface Props {
		headings: TocHeading[];
		/** The scrollable container to observe for scroll-spy */
		scrollContainer?: HTMLElement | null;
		/**
		 * Pin the active row instead of letting scroll position choose it.
		 * A contents over prose highlights what you are LOOKING at; a contents
		 * over steps highlights the one you are ON, which scrolling must not
		 * change.
		 */
		activeId?: string | null;
		/** Handle the click yourself — for rows that are not `#id` anchors. */
		onselect?: (id: string) => void;
		/**
		 * The prose gutter disappears on a narrow window, so the minimap goes
		 * with it. Chrome that keeps its corner passes false.
		 */
		hideWhenNarrow?: boolean;
		/**
		 * Which edge the lines hang from. In a right-hand gutter they grow
		 * rightward with the label after them; in the top-RIGHT corner that
		 * runs off the window, so `right` mirrors the whole row.
		 */
		align?: "left" | "right";
	}

	let {
		headings,
		scrollContainer = null,
		activeId: pinnedId = null,
		onselect,
		hideWhenNarrow = true,
		align = "left",
	}: Props = $props();

	let spiedId = $state<string | null>(null);
	let hoveredIndex = $state<number | null>(null);
	const activeId = $derived(pinnedId ?? spiedId);

	// Scroll-spy: track which section is currently in view
	$effect(() => {
		if (!browser || pinnedId !== null || !scrollContainer || headings.length === 0) return;

		const observer = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) {
					if (entry.isIntersecting) {
						spiedId = entry.target.id;
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
		if (onselect) return onselect(id);
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
		class:narrow-hides={hideWhenNarrow}
		class:mirrored={align === "right"}
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
					class:done={heading.state === "done"}
					class:skipped={heading.state === "skipped"}
					aria-current={activeId === heading.id ? "true" : undefined}
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
		.toc.narrow-hides {
			display: none;
		}
	}

	.toc-lines {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	/* Mirrored: the row reads label-then-line right-to-left, and the line
	   stretches toward the margin it hangs from rather than across the page. */
	.toc.mirrored .toc-lines {
		align-items: flex-end;
	}
	.toc.mirrored .toc-row {
		flex-direction: row-reverse;
	}
	.toc.mirrored .toc-line {
		transform-origin: right center;
	}
	.toc.mirrored .toc-row.is-h3 .toc-line {
		margin-left: 0;
		margin-right: 4px;
	}
	.toc.mirrored .toc-label {
		transform: translateX(6px);
	}
	.toc.mirrored .toc-row.hovered .toc-label {
		transform: translateX(0);
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

	/* A step already settled: stronger than untouched, quieter than the one
	   you are on. */
	.toc-row.done .toc-line {
		background: color-mix(in srgb, var(--color-foreground) 32%, transparent);
	}

	/* Set aside. Dashed, because it must not read as handled. */
	.toc-row.skipped .toc-line {
		background: repeating-linear-gradient(
			to right,
			color-mix(in srgb, var(--color-foreground) 30%, transparent) 0 3px,
			transparent 3px 6px
		);
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
