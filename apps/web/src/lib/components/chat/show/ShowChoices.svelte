<!--
	Replies the assistant offered, as buttons. A tap sends the words as your
	next message, exactly as if you had written them. Only the newest turn's
	buttons can be pressed; an older turn keeps them as a record of what was
	offered.
-->
<script lang="ts">
	import Act from "../getting-started/ui/Act.svelte";
	import Choices from "../getting-started/ui/Choices.svelte";

	interface Props {
		options: string[];
		active: boolean;
		onChoose?: (text: string) => void;
	}
	let { options, active, onChoose }: Props = $props();

	let chosen = $state<string | null>(null);

	function choose(text: string) {
		if (!active || chosen) return;
		chosen = text;
		onChoose?.(text);
	}
</script>

<Choices>
	{#each options as option}
		<Act variant={chosen === option ? "primary" : "quiet"} disabled={!active || !!chosen} onclick={() => choose(option)}>
			{option}
		</Act>
	{/each}
</Choices>
