<script lang="ts">
	import type { Snippet } from "svelte";

	/**
	 * A small standing fact about the thing beside it — a status, a count, a
	 * kind. Never a label you could have written as a word in the row.
	 *
	 * Two rules it was breaking, both from doctrine written before it:
	 *
	 * - It was set at **10px**, and design-grammar.md §4 says nothing under 11.
	 *   Eleven is the floor because ten is where a sans stops being read and
	 *   starts being recognized by shape.
	 * - It was set in **IBM Plex Mono**, and design.md §5 says mono is for code
	 *   and data. A status is neither. Mono kickers at 9.5–10.5px were named
	 *   there as "the single strongest tell of the dated register" — this was
	 *   the same move, one component further in.
	 *
	 * Color comes from the semantic tokens and nothing else, so a badge stays
	 * legible in all sixteen themes including Borghese, where `--color-primary`,
	 * `--color-success` and `--color-error` are deliberately all white. A badge
	 * that reads only by hue is unreadable there by construction; the word in
	 * it has to carry the meaning on its own.
	 */
	type Variant = "muted" | "success" | "error" | "warning" | "info" | "primary";

	let {
		variant = "muted",
		outline = false,
		uppercase = false,
		class: className = "",
		children,
	}: {
		variant?: Variant;
		/** Hairline and ink instead of a tinted ground. */
		outline?: boolean;
		uppercase?: boolean;
		class?: string;
		children: Snippet;
	} = $props();
</script>

<span
	class="v-badge {className}"
	data-variant={variant}
	data-fill={outline ? "outline" : "solid"}
	class:is-uppercase={uppercase}
>
	{@render children()}
</span>

<style>
	.v-badge {
		display: inline-flex;
		width: fit-content;
		align-items: center;
		gap: 4px;
		height: 20px;
		padding: 0 8px;
		border: 1px solid transparent;
		border-radius: 999px;
		font-family: var(--font-sans);
		font-size: 11px;
		font-weight: 500;
		line-height: 1.4;
		white-space: nowrap;
	}

	.is-uppercase {
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	/* Solid: a tinted ground from the -subtle token, ink from the full one.

	   `muted` is the exception and takes the ramp instead.
	   `--color-surface-elevated` is a 4% step over `--color-surface` at best,
	   and on Pemberley the two resolve to the same white — so a muted badge
	   rendered as bare text beside five tinted ones, which the gallery caught
	   the day it was built. The foreground mix is defined against the ink, so
	   it reads in all sixteen themes. design.md: never use a surface token
	   where a ground has to show. */
	[data-fill="solid"][data-variant="muted"] {
		background: var(--hover-bg);
		color: var(--color-foreground-muted);
	}
	[data-fill="solid"][data-variant="success"] {
		background: var(--color-success-subtle);
		color: var(--color-success);
	}
	[data-fill="solid"][data-variant="error"] {
		background: var(--color-error-subtle);
		color: var(--color-error);
	}
	[data-fill="solid"][data-variant="warning"] {
		background: var(--color-warning-subtle);
		color: var(--color-warning);
	}
	[data-fill="solid"][data-variant="info"] {
		background: var(--color-info-subtle);
		color: var(--color-info);
	}
	[data-fill="solid"][data-variant="primary"] {
		background: var(--color-primary-subtle);
		color: var(--color-primary);
	}

	/* Outline: hairline and ink, no ground. */
	[data-fill="outline"] {
		background: transparent;
	}
	[data-fill="outline"][data-variant="muted"] {
		border-color: var(--color-border);
		color: var(--color-foreground-muted);
	}
	[data-fill="outline"][data-variant="success"] {
		border-color: var(--color-success);
		color: var(--color-success);
	}
	[data-fill="outline"][data-variant="error"] {
		border-color: var(--color-error);
		color: var(--color-error);
	}
	[data-fill="outline"][data-variant="warning"] {
		border-color: var(--color-warning);
		color: var(--color-warning);
	}
	[data-fill="outline"][data-variant="info"] {
		border-color: var(--color-info);
		color: var(--color-info);
	}
	[data-fill="outline"][data-variant="primary"] {
		border-color: var(--color-primary);
		color: var(--color-primary);
	}
</style>
