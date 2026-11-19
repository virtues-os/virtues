<script lang="ts">
	/**
	 * One specimen placed in design.md: the live component in a frame, named
	 * by the file it comes from. A name the page cannot find says so, so a
	 * renamed or deleted specimen shows on the page instead of vanishing.
	 */
	import { specimens } from '$lib/design/specimens';

	let { name }: { name: string } = $props();

	const entry = $derived(specimens.get(name));
</script>

<figure class="specimen" id="specimen-{name}">
	<figcaption>
		<span class="name">{name}</span>
		{#if entry}<span class="path">{entry.path}</span>{/if}
	</figcaption>
	<div class="body">
		{#if entry}
			{#await entry.load()}
				<p class="note">Loading the specimen…</p>
			{:then mod}
				<mod.default />
			{:catch err}
				<p class="note">This specimen failed to load: {err?.message ?? err}</p>
			{/await}
		{:else}
			<p class="note">
				No specimen is named {name}. Add <code>{name}.specimen.svelte</code> beside its component,
				or fix the name in design.md.
			</p>
		{/if}
	</div>
</figure>

<style>
	.specimen {
		margin: 24px 0 32px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-page);
		overflow: hidden;
	}

	figcaption {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 16px;
		border-bottom: 1px solid var(--color-border);
		font-family: var(--font-sans);
		font-size: var(--text-caption);
		color: var(--color-foreground-subtle);
	}

	.name {
		color: var(--color-foreground-muted);
	}

	.path {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.body {
		padding: 24px;
		font-family: var(--font-sans);
		font-size: var(--text-label);
		color: var(--color-foreground);
	}

	.note {
		margin: 0;
		color: var(--color-foreground-muted);
	}
</style>
