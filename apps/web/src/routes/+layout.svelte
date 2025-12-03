<script lang="ts">
	// Root layout - minimal, delegates to route group layouts
	// This is kept minimal as (onboarding) and (app) groups have their own layouts
	let { children } = $props();
</script>

{@render children()}
