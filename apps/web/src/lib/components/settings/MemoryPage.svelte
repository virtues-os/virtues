<script lang="ts">
	/**
	 * Settings → the assistant → What it remembers: the notes list on a page
	 * of its own, because it grows (the rule in lib/settings: a list opens
	 * from a row).
	 */
	import { onMount } from 'svelte';
	import { Page } from '$lib';
	import MemorySettings from '$lib/components/MemorySettings.svelte';
	import { getAssistantProfile } from '$lib/api/client';
	import { findSetting, titleOf } from '$lib/settings';

	let name = $state('Ari');

	onMount(async () => {
		try {
			const p = await getAssistantProfile<{ assistant_name?: string | null }>();
			name = p.assistant_name?.trim() || 'Ari';
		} catch {
			// The default reads fine; the list below reports its own load error.
		}
	});

	const spoken = $derived(name.charAt(0).toUpperCase() + name.slice(1));
</script>

<Page title={titleOf(findSetting('memory')?.title ?? '', spoken)} maxWidth="prose">
	<MemorySettings assistantName={spoken} />
</Page>
