<!--
	Settings › the assistant's page › Compare every model (`/virtues/assistant/models`).

	The catalog has its own page because it is a table of a few hundred rows,
	and the Assistant page is a form. The Models section there links here.
-->
<script lang="ts">
	import ModelCatalog from "$lib/components/ModelCatalog.svelte";
	import SettingsPage from "$lib/components/settings/SettingsPage.svelte";
</script>

<SettingsPage
	title="Every model"
	lede="Every language model you can choose, with list prices per million tokens."
	back={{ route: "/virtues/assistant", label: "Assistant" }}
	width="full"
>
	<ModelCatalog />
</SettingsPage>
