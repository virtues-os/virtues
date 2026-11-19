<script lang="ts">
	/** TextAction on its own, quiet, loading, and inline in a sentence. */
	import TextAction from './TextAction.svelte';
</script>

<div class="stack">
	<div class="row">
		<TextAction>Open sources</TextAction>
		<TextAction quiet>Not now</TextAction>
		<TextAction loading loadingLabel="Saving…">Save</TextAction>
		<TextAction disabled>Open sources</TextAction>
	</div>
	<p>
		Your calendar has not synced since Tuesday. <TextAction inline>Reconnect it</TextAction> and the week
		fills in.
	</p>
</div>

<style>
	.stack {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		gap: 24px;
	}
	p {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-body);
	}
</style>
