<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import ModelSettings from '$lib/components/ModelSettings.svelte';
	import PersonaSettings from '$lib/components/PersonaSettings.svelte';
	import AssistantNameSettings from '$lib/components/AssistantNameSettings.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();
</script>

<Page title="Assistant" description="Configure AI models and behavior" maxWidth="prose">
	<div class="space-y-6">
		<AssistantNameSettings />
		<ModelSettings />
		<PersonaSettings />
	</div>
</Page>
