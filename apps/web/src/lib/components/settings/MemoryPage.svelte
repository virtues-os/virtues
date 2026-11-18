<script lang="ts">
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	/**
	 * Settings → the assistant → What it remembers: the notes list on a page
	 * of its own, because it grows (the rule in lib/settings: a list opens
	 * from a row).
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import MemorySettings from '$lib/components/MemorySettings.svelte';
	import { getAssistantProfile } from '$lib/api/client';
	import { findSetting, titleOf } from '$lib/settings';

	let name = $state(DEFAULT_ASSISTANT_NAME);

	onMount(async () => {
		try {
			const p = await getAssistantProfile<{ assistant_name?: string | null }>();
			name = p.assistant_name?.trim() || DEFAULT_ASSISTANT_NAME;
		} catch {
			// The default reads fine; the list below reports its own load error.
		}
	});

	const spoken = $derived(name.charAt(0).toUpperCase() + name.slice(1));
</script>

<SettingsPage
	title={titleOf(findSetting('memory')?.title ?? '', spoken)}
	lede="What {spoken} picked up in your chats and keeps in mind for the next one. A note you edit becomes yours, and {spoken} won't change it."
	back={{ route: '/virtues/assistant', label: spoken }}
	assistantName={spoken}
>
	<MemorySettings assistantName={spoken} />
</SettingsPage>
