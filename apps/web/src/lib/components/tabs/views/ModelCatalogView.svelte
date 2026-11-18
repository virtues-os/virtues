<!--
	Settings › Models and spending › the model list
	(`/virtues/assistant/spending/models`). A role's row there opens it with
	that role picked (`?role=standard`), so each model offers to be that
	role's model and nothing else.

	The list has its own page because it is a table of a few hundred rows.
-->
<script lang="ts">
	import ModelCatalog from "$lib/components/ModelCatalog.svelte";
	import SettingsPage from "$lib/components/settings/SettingsPage.svelte";
	import { modelSlots } from "$lib/stores/modelSlots.svelte";

	let { role }: { role?: string } = $props();

	const slot = $derived(role ? modelSlots.slots.find((s) => s.key === role && s.language) : undefined);
</script>

<SettingsPage
	title={slot ? `A model for ${slot.phrase}` : "Every model"}
	lede={slot
		? `Every language model you can choose for ${slot.phrase}, with list prices per million tokens.`
		: "Every language model you can choose, with list prices per million tokens."}
	back={{ route: "/virtues/assistant/spending", label: "Models and spending" }}
	width="full"
>
	<ModelCatalog role={slot?.key} />
</SettingsPage>
