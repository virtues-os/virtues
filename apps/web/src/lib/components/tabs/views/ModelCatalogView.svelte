<!--
	Settings > Assistant > All models (`/virtues/assistant/models`).

	The catalog has its own page because it is a table of a few hundred rows,
	and the Assistant page is a form. The Models section there links here.
	The way back sits above the title: the shell draws no back control.
-->
<script lang="ts">
	import { Button, Page, PageHeading } from "$lib";
	import ModelCatalog from "$lib/components/ModelCatalog.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";

	function backToAssistant() {
		windowShellStore.navigate("/virtues/assistant", { label: "Settings" });
	}
</script>

<Page maxWidth="wide">
	{#snippet heading()}
		<div class="back">
			<Button variant="ghost" size="sm" icon="ri:arrow-left-line" onclick={backToAssistant}>
				Back to Assistant
			</Button>
		</div>
		<PageHeading
			title="All models"
			description="Every language model you can choose, with list prices per million tokens."
		/>
	{/snippet}

	<ModelCatalog />
</Page>

<style>
	.back {
		margin-bottom: 8px;
	}
</style>
