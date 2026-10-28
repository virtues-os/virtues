<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';

	// Why the reply under this chip is partial. "stopped" is the person's own
	// stop button; "length" is the model's output window running out (the
	// stream's `finish` said so); "interrupted" is the stream or the model
	// quitting on them (a dropped connection, the idle timeout — VIR-334);
	// "unavailable" is the model's provider refusing the call while it was down;
	// "unattended" is the box stopping a reply nobody was watching, which used
	// to be reported as the person's own stop; "max_steps" is the turn using
	// up its allowance of tool calls.
	// Same chip, different word, because the difference is who to blame.
	let { reason = 'stopped' }: {
		reason?:
			| 'stopped'
			| 'length'
			| 'interrupted'
			| 'unavailable'
			| 'unattended'
			| 'max_steps'
			| 'budget'
			| 'no_reply';
	} = $props();
</script>

<div
	class="stopped-notice mt-2 inline-flex items-center gap-1.5 rounded-full border border-border-subtle bg-surface-elevated px-2.5 py-1 text-xs text-foreground-muted select-none"
	role="status"
>
	{#if reason === 'length'}
		<Icon icon="mynaui:scissors" width="13" />
		<span>Reached its output limit before it finished</span>
	{:else if reason === 'interrupted'}
		<Icon icon="mynaui:zap" width="13" />
		<span>Interrupted before it finished</span>
	{:else if reason === 'unavailable'}
		<Icon icon="mynaui:cloud-off" width="13" />
		<span>Your assistant's provider couldn't answer. Send it again to retry</span>
	{:else if reason === 'unattended'}
		<Icon icon="mynaui:moon" width="13" />
		<span>Stopped by your server. Nothing was watching</span>
	{:else if reason === 'no_reply'}
		<Icon icon="mynaui:power" width="13" />
		<span>Your server never finished this one</span>
	{:else if reason === 'max_steps'}
		<Icon icon="mynaui:activity" width="13" />
		<span>Used up its steps before it finished</span>
	{:else if reason === 'budget'}
		<!-- The turn's own ceiling of cost or time, checked between steps. -->
		<Icon icon="mynaui:clock-circle" width="13" />
		<span>Reached one turn's budget of time or cost before it finished</span>
	{:else}
		<Icon icon="mynaui:stop-solid" width="13" />
		<span>Stopped</span>
	{/if}
</div>
