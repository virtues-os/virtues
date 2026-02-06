<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import ModelSettings from '$lib/components/ModelSettings.svelte';
	import PersonaSettings from '$lib/components/PersonaSettings.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();
</script>

<Page>
	<div class="max-w-3xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Assistant</h1>
			<p class="text-foreground-muted">Configure AI models and behavior</p>
		</div>

		<div class="space-y-6">
			<ModelSettings />
			<PersonaSettings />
		</div>
	</div>
</Page>
