<script lang="ts">
	/**
	 * One setting on the Settings page: its name and one line of help on the
	 * left, its control on the right. `block` stacks the control under the
	 * words, for a field that needs the width (a note, a list, a picker).
	 *
	 * The id is the setting's id in `lib/settings`, so search and links can
	 * find the row (`data-setting`).
	 */
	import type { Snippet } from 'svelte';
	import { settingsFilter } from '$lib/settings/filter';

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

	const filter = settingsFilter();
</script>

<div class="row" class:block class:filtered={!filter.shows(id)} id="setting-{id}" data-setting={id}>
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

<style>
	/* Hidden while the page's search field doesn't match it. */
	.filtered {
		display: none !important;
	}

	.row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 24px;
		min-height: 58px;
		padding: 8px 0;
		border-bottom: 1px solid var(--color-border-subtle);
		box-sizing: border-box;
	}

	.row.block {
		grid-template-columns: minmax(0, 1fr);
		gap: 8px;
		padding: 16px 0;
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

	.row:not(.block) .control {
		justify-self: end;
	}
</style>
