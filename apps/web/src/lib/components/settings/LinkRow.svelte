<script lang="ts">
	/**
	 * A setting that opens its own page: a list, a chart, a table or a form
	 * too large for one row (`kind: 'page'` in `lib/settings`). The whole row
	 * is the button; the page opens in this tab, so Back returns here.
	 */
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let {
		id,
		label,
		help,
		href,
	}: {
		id: string;
		label: string;
		help?: string;
		href: string;
	} = $props();
</script>

<button
	type="button"
	class="row"
	id="setting-{id}"
	data-setting={id}
	onclick={() => windowShellStore.navigate(href, { label: `Settings · ${label}` })}
>
	<span class="words">
		<span class="name">{label}</span>
		{#if help}<span class="help">{help}</span>{/if}
	</span>
	<span class="open">
		Open
		<svg viewBox="0 0 24 24" aria-hidden="true" width="15" height="15">
			<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" />
		</svg>
	</span>
</button>

<style>
	.row {
		display: grid;
		grid-template-columns: minmax(0, 1fr) auto;
		align-items: center;
		gap: 24px;
		width: 100%;
		min-height: 56px;
		padding: 8px 0;
		border: none;
		border-bottom: 1px solid var(--color-border-subtle);
		background: none;
		text-align: left;
		font: inherit;
		color: inherit;
		cursor: pointer;
	}

	.name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.help {
		display: block;
		margin-top: 4px;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.open {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 14px;
		color: var(--color-foreground-muted);
		transition: color var(--motion-hover-out) var(--ease-premium);
	}

	.row:hover .open {
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}
</style>
