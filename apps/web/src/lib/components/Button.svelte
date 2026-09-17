<script lang="ts">
	import type { Snippet } from "svelte";
	import Icon from "$lib/components/Icon.svelte";

	/**
	 * The one button.
	 *
	 * It was written in Tailwind utilities (`px-3 py-1.5`, `rounded-lg`,
	 * `focus:ring-2 focus:ring-offset-2`) while 192 of the app's 227 components
	 * are written in scoped CSS against the theme tokens. That is the reason it
	 * had fourteen importers against 418 hand-rolled `<button>` elements: a
	 * developer working in a scoped-CSS file could not reach this button's
	 * styling from the stylesheet they were already writing, so they wrote
	 * their own. The fix was not a bigger API — it already took snippets. It
	 * was speaking the same language as everything around it.
	 *
	 * Shape is design-grammar.md §6: pills, 40px tall, `0 22px`, one filled
	 * claret action per page. The sizes are the 8pt grid either side of that.
	 *
	 * `variant` is a claim about the button's job, not its color:
	 *
	 *   primary    the one action the page is for. Claret. At most one per page.
	 *   secondary  a real action that is not the point of the page.
	 *   ghost      a control in chrome; nothing on the desk may out-shout the page.
	 *   danger     destroys something. Never also the primary.
	 *
	 * The ramp (`--hover-bg` / `--active-bg`) is a foreground mix, never a
	 * surface token — `--surface-elevated` against `--surface` is a 4% step
	 * that reads as nothing happening, and shipping it as a hover state is a
	 * bug this codebase has had twice.
	 *
	 * `loading` and `icon` exist because their absence was measured: the
	 * component gallery's API notes recorded "no `loading` and no `icon`", and
	 * every call site that needed a pending button wrote its own — `{importing
	 * ? 'Importing…' : 'Import'}` in one modal, `{working ? 'Turning on…' :
	 * 'Turn it on'}` in another, so two buttons doing the same thing announce
	 * it differently and neither tells assistive tech anything at all. The
	 * label swap also resizes the button mid-press, which is why loading here
	 * keeps the label in the box and lays the spinner over it.
	 *
	 * There is no icon-ONLY button, and that is the design, not an omission:
	 * `children` stays required, so the accessible name can never go missing.
	 * An `icon` with no label and no `aria-label` is a button that reads as
	 * "button" to a screen reader, and the only way to make that impossible to
	 * get wrong is to leave no way to express it.
	 */
	let {
		variant = "primary",
		size = "md",
		disabled = false,
		loading = false,
		icon,
		type = "button",
		onclick,
		class: className = "",
		children,
	}: {
		variant?: "primary" | "secondary" | "ghost" | "danger";
		size?: "sm" | "md" | "lg";
		disabled?: boolean;
		/** Working. Holds its size, stops taking clicks, and says `aria-busy`. */
		loading?: boolean;
		/** A leading iconify name, e.g. `ri:add-line`. Sizes with the label. */
		icon?: string;
		type?: "button" | "submit" | "reset";
		onclick?: (e: MouseEvent) => void;
		class?: string;
		children: Snippet;
	} = $props();
</script>

<button
	{type}
	class="v-btn {className}"
	data-variant={variant}
	data-size={size}
	data-loading={loading ? "true" : undefined}
	disabled={disabled || loading}
	aria-busy={loading || undefined}
	{onclick}
>
	<!-- The label stays in the box while loading — hidden with `opacity`, not
	     `visibility` or an `{#if}`, both of which would take the text out of the
	     accessible name and leave the button announcing itself as "button". -->
	<span class="v-btn-body">
		{#if icon}
			<Icon {icon} width="1em" height="1em" />
		{/if}
		{@render children()}
	</span>

	{#if loading}
		<span class="v-btn-spinner">
			<Icon icon="ri:loader-4-line" width="1em" height="1em" />
		</span>
	{/if}
</button>

<style>
	.v-btn {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		border: 1px solid transparent;
		border-radius: 999px;
		font-family: var(--font-sans);
		font-weight: 500;
		white-space: nowrap;
		cursor: pointer;
		transition:
			background-color 120ms ease,
			border-color 120ms ease,
			color 120ms ease;
	}

	.v-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	/* The label and its icon, as one flex row, so the spinner can be laid over
	   the whole of it without the button changing width. Swapping the label for
	   a spinner is what every hand-rolled pending button does, and it resizes
	   the button under the pointer that is still on it. */
	.v-btn-body {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
	}

	/* An inline SVG sits on the text baseline and hangs a descender's worth of
	   space below it; `block` takes it off the baseline so the flex row centers
	   the glyph against the label's x-height instead of against its own box. */
	.v-btn-body :global(svg) {
		display: block;
		flex-shrink: 0;
	}

	.v-btn[data-loading="true"] .v-btn-body {
		opacity: 0;
	}

	.v-btn-spinner {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.v-btn-spinner :global(svg) {
		display: block;
		animation: v-btn-spin 1s linear infinite;
	}

	/* Busy is not disabled. The button takes no clicks while it works, but the
	   `:disabled` dimming above would make a button that is DOING the thing
	   look like one that refuses to — so loading takes its color back. */
	.v-btn[data-loading="true"]:disabled {
		opacity: 1;
		cursor: progress;
	}

	@keyframes v-btn-spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	/* Keyboard only. A mouse press should not leave a ring behind it. */
	.v-btn:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}

	/* Sizes — 8pt grid, with the grammar's 40px in the middle. */
	.v-btn[data-size="sm"] {
		height: 32px;
		padding: 0 14px;
		font-size: 13px;
	}

	.v-btn[data-size="md"] {
		height: 40px;
		padding: 0 22px;
		font-size: 14px;
	}

	.v-btn[data-size="lg"] {
		height: 48px;
		padding: 0 28px;
		font-size: 15px;
	}

	/* Claret (`--color-secondary`) is the page's single filled action, and is
	   the one exception to `--color-primary` being the only color the pane
	   carries (design-grammar.md §5). Never a rule, a fill, a border for
	   emphasis, or a second button. */
	.v-btn[data-variant="primary"] {
		background: var(--color-secondary);
		color: var(--color-surface);
	}

	.v-btn[data-variant="primary"]:hover:not(:disabled) {
		background: var(--color-secondary-hover);
	}

	.v-btn[data-variant="primary"]:active:not(:disabled) {
		background: var(--color-secondary-active);
	}

	.v-btn[data-variant="secondary"] {
		background: var(--color-surface);
		border-color: var(--color-border);
		color: var(--color-foreground);
	}

	.v-btn[data-variant="secondary"]:hover:not(:disabled) {
		background: var(--hover-bg);
		border-color: var(--color-border-strong);
	}

	.v-btn[data-variant="secondary"]:active:not(:disabled) {
		background: var(--active-bg);
	}

	.v-btn[data-variant="ghost"] {
		background: transparent;
		color: var(--color-foreground-muted);
	}

	.v-btn[data-variant="ghost"]:hover:not(:disabled) {
		background: var(--hover-bg);
		color: var(--color-foreground);
	}

	.v-btn[data-variant="ghost"]:active:not(:disabled) {
		background: var(--active-bg);
	}

	.v-btn[data-variant="danger"] {
		background: var(--color-error);
		color: var(--color-surface);
	}

	.v-btn[data-variant="danger"]:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-error) 88%, black);
	}

	/* Keep the information, drop the travel (StateBlock.svelte does the same).
	   A spinner that cannot spin still has to read as "working", so it stays
	   drawn at full strength rather than being hidden with its animation. */
	@media (prefers-reduced-motion: reduce) {
		.v-btn {
			transition: none;
		}

		.v-btn-spinner :global(svg) {
			animation: none;
		}
	}
</style>
