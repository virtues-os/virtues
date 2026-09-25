<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import ModelSettings from '$lib/components/ModelSettings.svelte';
	import StyleNotesSettings from '$lib/components/StyleNotesSettings.svelte';
	import AssistantNameSettings from '$lib/components/AssistantNameSettings.svelte';
	import MemorySettings from '$lib/components/MemorySettings.svelte';
	import ModelCatalog from '$lib/components/ModelCatalog.svelte';

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
		<StyleNotesSettings />
		<MemorySettings />
		<!-- Last: it is the long one. The pickers in ModelSettings are the
		     summary; this is where the models get compared. -->
		<ModelCatalog />
	</div>
</Page>
