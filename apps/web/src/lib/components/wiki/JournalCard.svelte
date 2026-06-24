<script lang="ts">
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { getReflectionsForDate, createReflection, type Page } from '$lib/api/client';

	let { date }: { date: string } = $props();

	let reflections = $state<Page[]>([]);
	let loaded = $state(false);

	$effect(() => {
		const d = date;
		loaded = false;
		reflections = [];
		getReflectionsForDate(d).then((pages) => {
			reflections = pages;
			loaded = true;
		});
	});

	function openReflection(pageId: string) {
		windowShellStore.openTabFromRoute(`/page/${pageId}`);
	}

	async function addReflection() {
		const page = await createReflection(date);
		reflections = [...reflections, page];
		windowShellStore.openTabFromRoute(`/page/${page.id}`);
	}
</script>

{#if loaded && (reflections.length > 0 || true)}
	<div class="reflections-row">
		<span class="reflections-label">Your writing:</span>
		{#each reflections as reflection}
			<button class="reflection-chip" onclick={() => openReflection(reflection.id)}>
				{reflection.title}
			</button>
			{#if reflections.indexOf(reflection) < reflections.length - 1}
				<span class="chip-sep">&middot;</span>
			{/if}
		{/each}
		{#if reflections.length > 0}
			<span class="chip-sep">&middot;</span>
		{/if}
		<button class="reflection-create" onclick={addReflection}>+ New</button>
	</div>
{/if}

<style>
	.reflections-row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.375rem;
	}

	.reflection-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0;
		border: none;
		background: none;
		cursor: pointer;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		transition: color 0.15s;
	}

	.reflection-chip:hover {
		color: var(--color-foreground);
		text-decoration: underline;
	}

	.reflections-label {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		opacity: 0.6;
	}

	.chip-sep {
		color: var(--color-foreground-subtle);
		opacity: 0.4;
		font-size: 0.75rem;
	}

	.reflection-create {
		padding: 0;
		border: none;
		background: none;
		cursor: pointer;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		opacity: 0.6;
		transition: opacity 0.15s, color 0.15s;
	}

	.reflection-create:hover {
		opacity: 1;
		color: var(--color-foreground);
	}
</style>
