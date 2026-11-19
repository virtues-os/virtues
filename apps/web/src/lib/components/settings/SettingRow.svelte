<script lang="ts">
	/**
	 * One setting on a Settings page: its name in a column on the left, its
	 * control on the right, and one line of help under the control. Every
	 * Settings page uses this one layout, so a page reads as a column of names
	 * beside a column of answers.
	 *
	 * `block` makes the row a section of the page: its name is the section's
	 * heading, its help under that, and the control (a list, a picker, a note)
	 * takes the full width below.
	 *
	 * The id is the setting's id in `lib/settings`, so search and links can
	 * find the row (`data-setting`).
	 */
	import type { Snippet } from 'svelte';

	let {
		id,
		label,
		help,
		forId,
		block = false,
		children,
	}: {
		id: string;
		label: string;
		help?: string;
		/** The control's id, so the name is its label. */
		forId?: string;
		block?: boolean;
		children: Snippet;
	} = $props();
</script>

{#if block}
	<div class="row block" id="setting-{id}" data-setting={id}>
		<div class="words">
			{#if forId}
				<label class="name" for={forId}>{label}</label>
			{:else}
				<span class="name">{label}</span>
			{/if}
			{#if help}<p class="help">{help}</p>{/if}
		</div>
		<div class="control">{@render children()}</div>
	</div>
{:else}
	<div class="row" id="setting-{id}" data-setting={id}>
		{#if forId}
			<label class="name" for={forId}>{label}</label>
		{:else}
			<span class="name">{label}</span>
		{/if}
		<div class="answer">
			<div class="control">{@render children()}</div>
			{#if help}<p class="help">{help}</p>{/if}
		</div>
	</div>
{/if}

<style>
	.row {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr);
		align-items: start;
		gap: 16px;
		padding: 12px 0;
		box-sizing: border-box;
	}

	/* The name sits on the control's first line. */
	.row:not(.block) .name {
		padding-top: 6px;
	}

	.answer {
		min-width: 0;
	}

	.answer .control {
		display: flex;
		align-items: center;
		min-height: 34px;
	}

	.answer .help {
		margin: 6px 0 0;
		font-size: 12px;
	}

	/* A block is a section of the page: a list, a picker, a note. Its name
	   is the section's heading, the app's serif h2 at the heading step. */
	.row.block {
		grid-template-columns: minmax(0, 1fr);
		gap: 8px;
		padding: 24px 0 8px;
	}

	.row.block .name {
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		line-height: 1.3;
	}

	.name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.help {
		margin: 4px 0 0;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.control {
		min-width: 0;
	}

	@media (max-width: 560px) {
		.row:not(.block) {
			grid-template-columns: minmax(0, 1fr);
			gap: 6px;
		}

		.row:not(.block) .name {
			padding-top: 0;
		}
	}
</style>
