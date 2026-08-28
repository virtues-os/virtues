<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import ModelSettings from '$lib/components/ModelSettings.svelte';
	import PersonaSettings from '$lib/components/PersonaSettings.svelte';
	import AssistantNameSettings from '$lib/components/AssistantNameSettings.svelte';
	import MemorySettings from '$lib/components/MemorySettings.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();
</script>

<!--
	`wide`, like every other room (Chat History, Sources, Usage, Box). The
	settings sections were the only pages measured at `prose`, which made the
	column jump width every time you moved between a settings row and anything
	else — for pages that are forms and lists, not reading.
-->
<Page title="Assistant" description="Configure AI models and behavior" maxWidth="wide">
	<div class="space-y-6">
		<AssistantNameSettings />
		<ModelSettings />
		<PersonaSettings />
		<MemorySettings />
	</div>
</Page>
