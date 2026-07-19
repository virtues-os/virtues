<!--
	PageOutline.svelte

	Table of contents, "Library drawer" style: a slim bar showing the current
	section; click it and a panel drops down with the full h1–h3 outline (active
	entry emphasized, the rest dimmed), over a blurred navy scrim. Click an entry
	to scroll the editor there. Scroll-spy keeps the bar's label and the active
	entry in sync as you read.

	Works against the CodeMirror view directly (headings live as lines, not DOM
	anchors): scroll-to via EditorView.scrollIntoView, scroll-spy off the editor's
	own scroller.
-->
<script lang="ts">
	import { EditorView } from "@codemirror/view";
	import Icon from "$lib/components/Icon.svelte";
	import type { PageHeading } from "$lib/codemirror/outline";

	let { headings, view }: { headings: PageHeading[]; view: EditorView | null } = $props();

	let open = $state(false);
	let activeFrom = $state<number | null>(null);

	const activeHeading = $derived(
		headings.find((h) => h.from === activeFrom) ?? headings[0] ?? null,
	);

	function scrollTo(h: PageHeading) {
		open = false;
		if (!view) return;
		const pos = Math.min(h.from, view.state.doc.length);
		view.dispatch({ effects: EditorView.scrollIntoView(pos, { y: "start", yMargin: 24 }) });
		activeFrom = h.from;
	}

	// The active section = the last heading whose line has scrolled to/above the
	// top of the viewport (with a small threshold so it flips a touch early).
	function computeActive() {
		if (!view || headings.length === 0) return;
		const threshold = view.scrollDOM.scrollTop + 96;
		let current: number | null = headings[0].from;
		for (const h of headings) {
			const pos = Math.min(h.from, view.state.doc.length);
			let top: number;
			try {
				top = view.lineBlockAt(pos).top;
			} catch {
				continue;
			}
			if (top <= threshold) current = h.from;
			else break;
		}
		activeFrom = current;
	}

	// Scroll-spy off the editor's scroller.
	$effect(() => {
		if (!view) return;
		const scroller = view.scrollDOM;
		computeActive();
		const onScroll = () => computeActive();
		scroller.addEventListener("scroll", onScroll, { passive: true });
		return () => scroller.removeEventListener("scroll", onScroll);
	});

	// Recompute when the outline itself changes (edits add/remove headings).
	$effect(() => {
		void headings;
		computeActive();
	});
</script>

{#if headings.length > 0}
	<div class="page-outline">
		<button class="outline-bar" onclick={() => (open = !open)} aria-expanded={open}>
			<span class="outline-current">{activeHeading?.text ?? "Contents"}</span>
			<Icon icon={open ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"} width="18" />
		</button>

		{#if open}
			<button class="outline-scrim" aria-label="Close outline" onclick={() => (open = false)}></button>
			<nav class="outline-drawer" aria-label="Table of contents">
				{#each headings as h}
					<button
						class="outline-item level-{h.level}"
						class:active={h.from === activeFrom}
						onclick={() => scrollTo(h)}
					>
						{h.text}
					</button>
				{/each}
			</nav>
		{/if}
	</div>
{/if}

<style>
	/* No box of its own — the trigger sits inline in the top bar, the drawer
	   anchors to the bar (.page-topbar is the positioned ancestor). */
	.page-outline {
		display: contents;
	}

	/* Centered in the top bar (icons sit at the far right of the same row). */
	.outline-bar {
		all: unset;
		box-sizing: border-box;
		position: absolute;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		z-index: var(--z-sticky);
		display: inline-flex;
		align-items: center;
		gap: 3px;
		max-width: 42ch;
		padding: 5px 8px 5px 12px;
		border-radius: 7px;
		cursor: pointer;
		font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
		transition:
			color 0.12s ease,
			background 0.12s ease;
	}
	.outline-bar:hover {
		color: var(--color-foreground);
		background: var(--color-surface-elevated, color-mix(in srgb, var(--color-foreground) 6%, transparent));
	}
	.outline-current {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* Blurred navy scrim over just the PAGE content (below the bar) — not the app
	   sidebar/chrome. Anchored to .page-topbar, which spans only the page column,
	   and starting at the bar's bottom so the bar itself stays clear. */
	.outline-scrim {
		all: unset;
		position: absolute;
		top: 100%;
		left: 0;
		right: 0;
		height: 100vh;
		z-index: var(--z-sticky);
		cursor: default;
		background: rgba(2, 6, 23, 0.32);
		backdrop-filter: blur(2px);
		-webkit-backdrop-filter: blur(2px);
		animation: outline-fade 0.14s ease-out;
	}

	.outline-drawer {
		position: absolute;
		top: 100%;
		left: 0;
		right: 0;
		z-index: var(--z-sticky);
		display: flex;
		flex-direction: column;
		padding: 0.4rem 1.25rem 0.7rem;
		background: var(--color-surface);
		border-bottom: 1px solid var(--color-border);
		box-shadow: 0 14px 28px -10px rgba(0, 0, 0, 0.3);
		max-height: 60vh;
		overflow-y: auto;
		animation: outline-drop 0.16s cubic-bezier(0.22, 1, 0.36, 1);
	}

	.outline-item {
		all: unset;
		box-sizing: border-box;
		cursor: pointer;
		padding: 0.28rem 0.5rem;
		border-radius: 5px;
		font-family: var(--font-sans, ui-sans-serif, system-ui, sans-serif);
		font-size: 0.875rem;
		line-height: 1.35;
		color: var(--color-foreground-subtle);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		transition:
			color 0.12s ease,
			background 0.12s ease;
	}
	.outline-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		color: var(--color-foreground-muted);
	}
	.outline-item.active {
		color: var(--color-foreground);
		font-weight: 500;
	}

	.level-1 { padding-left: 0.5rem; }
	.level-2 { padding-left: 1.5rem; }
	.level-3 { padding-left: 2.5rem; }

	@keyframes outline-drop {
		from { opacity: 0; transform: translateY(-6px); }
		to { opacity: 1; transform: translateY(0); }
	}
	@keyframes outline-fade {
		from { opacity: 0; }
		to { opacity: 1; }
	}
</style>
