<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
</script>

<div
	class="stopped-notice mt-2 inline-flex items-center gap-1.5 rounded-full border border-border-subtle bg-surface-elevated px-2.5 py-1 text-xs text-foreground-muted select-none"
	role="status"
>
	<Icon icon="ri:stop-fill" width="13" />
	<span>Stopped</span>
</div>
