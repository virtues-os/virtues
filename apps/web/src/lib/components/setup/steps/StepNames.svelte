<!--
	Names, the step after the server. One exchange and one place: its name, then yours ("And
	what should Virtues call you?"), then what excites you most about it,
	then home on the city chart.

	Four beats, one dot. The names are one conversation — the assistant is
	named first so the questions about you can be asked in its name — and
	home belongs with them because it is the last thing a day needs to know
	whose it is: the time zone the day turns over in. The step is done once
	the person has said what to be called. What excites you most is
	optional, and home refines a zone the browser already supplied, so
	neither holds the step open.
-->
<script lang="ts">
	import StepName from "./StepName.svelte";
	import StepFirst from "./StepFirst.svelte";
	import StepLocation from "./StepLocation.svelte";

	let { onnext }: { onnext: () => void } = $props();

	let beat = $state<"names" | "first" | "home">("names");
</script>

{#if beat === "names"}
	<StepName onnext={() => (beat = "first")} />
{:else if beat === "first"}
	<StepFirst onnext={() => (beat = "home")} onback={() => (beat = "names")} />
{:else}
	<StepLocation {onnext} onback={() => (beat = "first")} />
{/if}
