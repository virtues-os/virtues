<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import type { Snippet } from "svelte";

	/**
	 * One row of a menu — the thing you click inside a popover, a dropdown, or
	 * the context menu.
	 *
	 * It already existed, once, as the markup inside
	 * `contextMenu/ContextMenuItem.svelte`. But that component takes a
	 * `ContextMenuItem` from the context-menu STORE and talks to the store on
	 * click, so nothing outside the context menu could use it — and six other
	 * menus re-rolled the row under six names: `menu-item`, `menu-opt`,
	 * `popover-row`, `overflow-item`, `new-menu-item`, `add-row`.
	 *
	 * So the row moved here and `ContextMenuItem` became an adapter over it:
	 * it still owns the store, the submenu opening and the keyboard focus, and
	 * it renders this. One definition, two kinds of caller.
	 *
	 * THREE PARTS, because the call sites only ever had three:
	 *
	 *   leading    an icon (`icon`), or anything else (`leading` snippet) —
	 *              the filter chip needs a colored dot and a checkbox, neither
	 *              of which is an iconify name.
	 *   words      `label`, and `description` beneath it when the row is
	 *              explaining a choice rather than naming one (the applet
	 *              "New" menu is three of these).
	 *   trailing   `shortcut`, the `checked` tick, the `submenu` chevron, or
	 *              anything else (`trailing` snippet).
	 *
	 * `checked` is the state, not a picture: it paints the row's ground AND
	 * renders the tick, so a caller stops hand-drawing `ri:check-line` beside
	 * a `class:on`. Pass `role="menuitemcheckbox"` or `"menuitemradio"` when
	 * the menu is a set of choices rather than a list of commands — `checked`
	 * then reports `aria-checked` instead of `aria-current`, which is the
	 * difference between "this is selected" and "you are here".
	 *
	 * The row does NOT close its own menu. Closing belongs to whatever owns
	 * the popover, and a row that closed itself would be wrong inside a
	 * multi-select filter. Every call site already passes `close()` in its
	 * handler; that stays.
	 */
	let {
		icon,
		leading,
		label,
		description,
		shortcut,
		trailing,
		checked = false,
		destructive = false,
		disabled = false,
		loading = false,
		submenu = false,
		expanded,
		focused = false,
		role = "menuitem",
		onclick,
		onmouseenter,
		onmouseleave,
		class: className = "",
	}: {
		/** An iconify name for the common leading glyph. */
		icon?: string;
		/** A leading element that is not an icon — a dot, a checkbox. Wins over `icon`. */
		leading?: Snippet;
		label: string;
		/** A second line, when the row explains a choice rather than naming one. */
		description?: string;
		shortcut?: string;
		trailing?: Snippet;
		/** Selected. Paints the ground and renders the tick; reports to the role. */
		checked?: boolean;
		destructive?: boolean;
		disabled?: boolean;
		loading?: boolean;
		/** Renders the chevron and `aria-haspopup="menu"`. */
		submenu?: boolean;
		expanded?: boolean;
		/** Keyboard cursor — the owner's business, not the row's. */
		focused?: boolean;
		role?: "menuitem" | "menuitemcheckbox" | "menuitemradio" | "option";
		onclick?: (e: MouseEvent) => void;
		onmouseenter?: (e: MouseEvent) => void;
		onmouseleave?: (e: MouseEvent) => void;
		class?: string;
	} = $props();

	// A command menu says "you are here"; a set of choices says "this one is
	// selected". Same tick, different sentence to a screen reader.
	const isChoice = $derived(role === "menuitemcheckbox" || role === "menuitemradio" || role === "option");
</script>

<button
	type="button"
	{role}
	class="v-menuitem {className}"
	data-destructive={destructive ? "true" : undefined}
	class:is-checked={checked}
	class:is-focused={focused}
	disabled={disabled || loading}
	aria-disabled={disabled || undefined}
	aria-checked={isChoice ? checked : undefined}
	aria-current={!isChoice && checked ? "true" : undefined}
	aria-haspopup={submenu ? "menu" : undefined}
	aria-expanded={submenu ? expanded : undefined}
	aria-busy={loading || undefined}
	{onclick}
	{onmouseenter}
	{onmouseleave}
>
	{#if leading}
		<span class="mi-lead">{@render leading()}</span>
	{:else if loading}
		<span class="mi-lead mi-spin"><Icon icon="ri:loader-4-line" width="16" /></span>
	{:else if icon}
		<span class="mi-lead"><Icon {icon} width="16" /></span>
	{/if}

	<span class="mi-words">
		<span class="mi-label">{label}</span>
		{#if description}
			<span class="mi-desc">{description}</span>
		{/if}
	</span>

	{#if trailing}
		<span class="mi-trail">{@render trailing()}</span>
	{:else if shortcut}
		<span class="mi-trail mi-shortcut">{shortcut}</span>
	{:else if checked}
		<span class="mi-trail"><Icon icon="ri:check-line" width="14" /></span>
	{/if}

	{#if submenu}
		<span class="mi-trail mi-chevron"><Icon icon="ri:arrow-right-s-line" width="16" /></span>
	{/if}
</button>

<style>
	.v-menuitem {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 6px 10px;
		border: none;
		border-radius: 6px;
		background: transparent;
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 13px;
		text-align: left;
		cursor: pointer;
		transition: background-color 100ms ease;
	}

	/* The interaction ramp, never a surface token: `--surface-elevated` against
	   `--surface` is a 4% step that reads as nothing happening, and on some
	   themes they resolve to the same ink. design.md has this shipping twice. */
	.v-menuitem:hover:not(:disabled),
	.v-menuitem.is-focused:not(:disabled) {
		background: var(--hover-bg);
	}

	.v-menuitem:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.v-menuitem:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: -2px;
	}

	.v-menuitem.is-checked {
		background: var(--active-bg);
	}

	[data-destructive="true"] {
		color: var(--color-error);
	}

	[data-destructive="true"]:hover:not(:disabled),
	[data-destructive="true"].is-focused:not(:disabled) {
		background: color-mix(in srgb, var(--color-error) 12%, transparent);
	}

	.mi-lead {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		color: var(--color-foreground-muted);
	}

	[data-destructive="true"] .mi-lead {
		color: var(--color-error);
	}

	/* A two-line row puts its glyph on the first line, not in the middle of
	   the pair — the eye reads the glyph against the title it belongs to. */
	.v-menuitem:has(.mi-desc) {
		align-items: flex-start;
	}
	.v-menuitem:has(.mi-desc) .mi-lead {
		margin-top: 1px;
	}

	.mi-words {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.mi-label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.mi-desc {
		font-size: 12px;
		line-height: 1.35;
		color: var(--color-foreground-muted);
		white-space: normal;
	}

	.mi-trail {
		flex-shrink: 0;
		display: flex;
		align-items: center;
		color: var(--color-foreground-muted);
	}

	.mi-shortcut {
		font-size: 11px;
		opacity: 0.7;
	}

	.mi-chevron {
		margin-left: auto;
	}

	.mi-spin :global(svg) {
		animation: mi-spin 1s linear infinite;
	}

	@keyframes mi-spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* Keep the information, drop the travel. A spinner that cannot spin still
	   has to read as "working". */
	@media (prefers-reduced-motion: reduce) {
		.v-menuitem {
			transition: none;
		}
		.mi-spin :global(svg) {
			animation: none;
		}
	}
</style>
