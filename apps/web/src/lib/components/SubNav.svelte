<script module lang="ts">
	export interface SubNavItem {
		id: string;
		label: string;
		/** Optional trailing chrome (count, dot). Snippet `item` overrides this. */
		badge?: string | number;
	}
</script>

<script lang="ts">
	import { tick, type Snippet } from "svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";

	let {
		tabId,
		route,
		base,
		default: defaultId,
		items,
		insetX = "1.5rem",
		divider = false,
		ariaLabel = "Sections",
		item,
	}: {
		/** The owning tab — the route lives on the pane, so sub-nav is per-pane. */
		tabId: string;
		/** The pane's current route. `active` is DERIVED from this, never stored. */
		route: string;
		/** Parent route. The only URLs this can produce are `${base}` or `${base}/${id}`. */
		base: string;
		/** Which id the bare `base` route resolves to. */
		default: string;
		items: SubNavItem[];
		/** Horizontal inset. Full-bleed panes want `1.5rem`; inside a padded column, `0`. */
		insetX?: string;
		/** Draw a hairline seam under the row (to separate from content that follows). */
		divider?: boolean;
		ariaLabel?: string;
		/** Optional custom item chrome (label + badge + icon). Chrome only — never content. */
		item?: Snippet<[SubNavItem, boolean]>;
	} = $props();

	// The active segment is a pure function of the route. Because the only thing we
	// ever write is `base` or `base/<child-id>`, a sub-nav can only point at a child
	// of its own room — never off to another destination.
	const validIds = $derived(new Set(items.map((i) => i.id)));
	const active = $derived.by(() => {
		if (route === base) return defaultId;
		const rest = route.startsWith(base + "/") ? route.slice(base.length + 1) : "";
		return validIds.has(rest) ? rest : defaultId;
	});

	function switchTo(id: string) {
		const next = id === defaultId ? base : `${base}/${id}`;
		if (route === next) return;
		windowShellStore.updateTab(tabId, { route: next });
	}

	// Sliding underline, declarative: one effect reads the active button's box and
	// writes two CSS vars; the line itself transitions in CSS. No ResizeObserver, no
	// motion lib, no per-button ref map. Button offsets are text-intrinsic and
	// left-aligned, so they don't shift when the pane resizes — measuring on
	// active-change (plus once when webfonts settle) is enough.
	let navEl: HTMLElement | null = $state(null);
	let ready = $state(false);

	async function measure() {
		if (!navEl) return;
		await tick();
		const btn = navEl.querySelector<HTMLButtonElement>("button.is-active");
		if (!btn) return;
		navEl.style.setProperty("--ux", `${btn.offsetLeft}px`);
		navEl.style.setProperty("--uw", `${btn.offsetWidth}px`);
	}

	$effect(() => {
		void active;
		void measure().then(() => {
			if (!ready) ready = true;
		});
	});

	$effect(() => {
		const fonts = (document as unknown as { fonts?: { ready?: Promise<unknown> } }).fonts;
		void fonts?.ready?.then(() => measure());
	});
</script>

<nav
	class="subnav"
	class:ready
	class:divider
	style="--inset-x: {insetX}"
	bind:this={navEl}
	aria-label={ariaLabel}
>
	{#each items as it (it.id)}
		<button
			type="button"
			class:is-active={active === it.id}
			aria-current={active === it.id ? "page" : undefined}
			onclick={() => switchTo(it.id)}
		>
			{#if item}
				{@render item(it, active === it.id)}
			{:else}
				{it.label}
				{#if it.badge != null && it.badge !== ""}
					<span class="badge">{it.badge}</span>
				{/if}
			{/if}
		</button>
	{/each}
	<span class="underline" aria-hidden="true"></span>
</nav>

<style>
	/* One look everywhere — the row is chrome, not content, so it reads the same
	   on the workbench and on a reflective page. Only the inset and an optional
	   seam vary (both structural). The serif register belongs to page content. */
	.subnav {
		position: relative;
		display: flex;
		flex-shrink: 0;
		gap: 1.25rem;
		padding: 0.5rem var(--inset-x, 1.5rem) 0;
	}

	.subnav.divider {
		border-bottom: 1px solid var(--color-border-subtle);
		margin-bottom: 1.75rem;
	}
	.subnav.divider button {
		padding-bottom: 0.625rem;
	}

	.subnav button {
		font: inherit;
		font-size: 0.8125rem;
		font-weight: 500;
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.25rem 0;
		background: transparent;
		border: none;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		transition: color 120ms ease;
	}

	.subnav button:hover {
		color: var(--color-foreground-muted);
	}

	.subnav button.is-active {
		color: var(--color-foreground);
	}

	.badge {
		font-size: 0.6875em;
		font-weight: 500;
		line-height: 1;
		padding: 0.125rem 0.3125rem;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-muted);
	}

	.underline {
		position: absolute;
		bottom: 0;
		left: 0;
		height: 1px;
		width: var(--uw, 0);
		transform: translateX(var(--ux, 0));
		background: var(--color-foreground);
		pointer-events: none;
	}

	/* Only animate once the first position is set, so it doesn't slide in from 0. */
	.subnav.ready .underline {
		transition:
			transform 220ms cubic-bezier(0.32, 0.72, 0, 1),
			width 220ms cubic-bezier(0.32, 0.72, 0, 1);
	}
</style>
