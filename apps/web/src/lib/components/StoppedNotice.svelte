<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';

	// Why the reply under this chip is partial. "stopped" is the person's own
	// stop button; "interrupted" is the stream or the model quitting on them
	// (a dropped connection, the idle timeout, the output cap — VIR-334). Same
	// chip, different word, because the difference is who to blame.
	let { reason = 'stopped' }: { reason?: 'stopped' | 'interrupted' } = $props();
</script>

<div
	class="stopped-notice mt-2 inline-flex items-center gap-1.5 rounded-full border border-border-subtle bg-surface-elevated px-2.5 py-1 text-xs text-foreground-muted select-none"
	role="status"
>
	{#if reason === 'interrupted'}
		<Icon icon="ri:flashlight-line" width="13" />
		<span>Interrupted before it finished</span>
	{:else}
		<Icon icon="ri:stop-fill" width="13" />
		<span>Stopped</span>
	{/if}
</div>
