<script lang="ts">
	// The recovery screens: the pages a paired device opens when its server
	// isn't answering. Their own group because every other group asks the
	// server something before it draws (the session, the profile), and here
	// that is exactly the question with no answer. They run from the app's own
	// copy: the phone's baked bundle, the Mac's baked copy.
	import "../../app.css";
	import "$lib/components/onboarding/onboarding.css";
	import "$lib/components/setup/setup.css";
	import { onMount } from "svelte";
	import { initTheme } from "$lib/utils/theme";

	let { children } = $props();

	onMount(() => {
		initTheme();
	});
</script>

<div class="min-h-screen overscroll-none bg-surface text-foreground">
	{@render children?.()}
</div>
