<script lang="ts">
	/**
	 * The mark a thing wears in every view of the Inbox, so one glance says
	 * what kind it is: a filled dot waits on you, ∴ is something your server
	 * noticed, a dashed ring is a task of yours, and a dotted ring is a
	 * request the assistant has not finished.
	 */
	import Icon from '$lib/components/Icon.svelte';
	import type { ItemKind } from '$lib/inbox/items';

	let { kind }: { kind: ItemKind } = $props();
</script>

<span class="mark {kind}" aria-hidden="true">{#if kind === 'insight'}<Icon icon="virtues:logo" width="18" height="18" />{/if}</span>

<style>
	.mark {
		display: inline-block;
		flex: none;
		box-sizing: border-box;
		border-radius: 50%;
		vertical-align: middle;
	}
	.needs {
		width: 14px;
		height: 14px;
		background: var(--color-primary);
	}
	.insight {
		display: inline-grid;
		place-items: center;
		width: 22px;
		height: 22px;
		color: var(--color-primary);
	}
	.task {
		width: 14px;
		height: 14px;
		border: 2px dashed var(--color-foreground);
		background: var(--color-page);
	}
	.waiting {
		width: 12px;
		height: 12px;
		border: 2px dotted var(--color-primary);
		background: var(--color-page);
	}
</style>
