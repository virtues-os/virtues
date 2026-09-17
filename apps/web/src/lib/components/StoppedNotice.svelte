<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';

	// Why the reply under this chip is partial. "stopped" is the person's own
	// stop button; "length" is the model's output window running out (the
	// stream's `finish` said so); "interrupted" is the stream or the model
	// quitting on them (a dropped connection, the idle timeout — VIR-334).
	// Same chip, different word, because the difference is who to blame.
	let { reason = 'stopped' }: { reason?: 'stopped' | 'length' | 'interrupted' } = $props();
</script>

<div
	class="stopped-notice mt-2 inline-flex items-center gap-1.5 rounded-full border border-border-subtle bg-surface-elevated px-2.5 py-1 text-xs text-foreground-muted select-none"
	role="status"
>
	{#if reason === 'length'}
		<Icon icon="ri:scissors-cut-line" width="13" />
		<span>Reached its output limit before it finished</span>
	{:else if reason === 'interrupted'}
		<Icon icon="ri:flashlight-line" width="13" />
		<span>Interrupted before it finished</span>
	{:else}
		<Icon icon="ri:stop-fill" width="13" />
		<span>Stopped</span>
	{/if}
</div>
