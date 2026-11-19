<script lang="ts">
	/**
	 * The app's one tooltip: `data-tip` on the element, drawn by the
	 * TooltipLayer the app layout mounts once. Use it only where the words
	 * are hidden or cut off.
	 */
	import IconButton from './IconButton.svelte';
</script>

<div class="row">
	<span data-tip="New chat" data-tip-key="mod+n"><IconButton icon="mynaui:plus" label="New chat" /></span>
	<span class="clipped" data-tip-clipped>
		<span data-tip-text>A page title long enough that the row cuts it off before the end</span>
	</span>
</div>

<style>
	.row {
		display: flex;
		align-items: center;
		gap: 24px;
		font-size: var(--text-label);
	}
	.clipped {
		max-width: 200px;
		overflow: hidden;
		white-space: nowrap;
	}
	.clipped [data-tip-text] {
		display: block;
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
