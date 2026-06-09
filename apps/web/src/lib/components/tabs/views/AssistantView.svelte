<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import ModelSettings from '$lib/components/ModelSettings.svelte';
	import CouncilSettings from '$lib/components/CouncilSettings.svelte';
	import PersonaSettings from '$lib/components/PersonaSettings.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();
</script>

<Page title="Assistant" description="Configure AI models and behavior" maxWidth="prose">
	<div class="space-y-6">
		<ModelSettings />
		<CouncilSettings />
		<PersonaSettings />
	</div>
</Page>
