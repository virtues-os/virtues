<script lang="ts">
	/**
	 * Every accent swatch in the theme on screen, each with a link and the
	 * filled button it would draw. A row carries its swatch's `data-accent`
	 * and re-points --color-primary at it, so the preview never changes the
	 * accent you chose.
	 */
	import Button from '$lib/components/Button.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { ACCENTS } from '$lib/utils/theme';
</script>

<div class="rows">
	{#each ACCENTS as accent (accent.id)}
		<div class="row" data-accent={accent.id}>
			<span class="dot"></span>
			<span class="name">{accent.name}</span>
			<TextAction>Open sources</TextAction>
			<Button>Save</Button>
		</div>
	{/each}
</div>

<style>
	.rows {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.row {
		--color-primary: var(--accent-light);
		--color-primary-hover: color-mix(in srgb, var(--accent-light) 88%, var(--foreground));
		--color-primary-active: color-mix(in srgb, var(--accent-light) 78%, var(--foreground));
		display: grid;
		grid-template-columns: 24px 96px max-content max-content;
		align-items: center;
		gap: 16px;
	}
	:global([data-theme="asgard"]) .row {
		--color-primary: var(--accent-dark);
		--color-primary-hover: color-mix(in srgb, var(--accent-dark) 88%, var(--foreground));
		--color-primary-active: color-mix(in srgb, var(--accent-dark) 78%, var(--foreground));
	}
	.dot {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: var(--color-primary);
	}
	.name {
		font-size: var(--text-label);
		color: var(--color-foreground-muted);
	}
</style>
