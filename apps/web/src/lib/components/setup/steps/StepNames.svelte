<!--
	Step 4 — Names. One exchange and one place: its name, then yours ("And
	what should Ari call you?"), then home on the city chart.

	Three beats, one dot. The names are one conversation — the assistant is
	named first so the question about you can be asked in its name — and
	home belongs with them because it is the last thing a day needs to know
	whose it is: the time zone the day turns over in. The step is done once
	the person has said what to be called; home refines a zone the browser
	already supplied, so it never holds the step open.
-->
<script lang="ts">
	import StepName from "./StepName.svelte";
	import StepLocation from "./StepLocation.svelte";

	let { onnext }: { onnext: () => void } = $props();

	let beat = $state<"names" | "home">("names");
</script>

{#if beat === "names"}
	<StepName onnext={() => (beat = "home")} />
{:else}
	<StepLocation {onnext} onback={() => (beat = "names")} />
{/if}
