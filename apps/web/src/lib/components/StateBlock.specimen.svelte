<script lang="ts">
	/** EmptyState, LoadingState and ErrorState, the three faces of StateBlock. */
	import EmptyState from './EmptyState.svelte';
	import ErrorState from './ErrorState.svelte';
	import LoadingState from './LoadingState.svelte';
</script>

<div class="grid">
	<EmptyState title="No pages yet" message="A page you write or keep from a chat shows here." />
	<LoadingState message="Reading your calendar" />
	<ErrorState message="Your server did not answer. Check that it is on, then try again." onRetry={() => {}} />
</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 24px;
	}
</style>
