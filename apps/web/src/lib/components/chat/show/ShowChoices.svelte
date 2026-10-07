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
		onChoose?: (text: string) => Promise<boolean>;
	}
	let { options, active, onChoose }: Props = $props();

	let chosen = $state<string | null>(null);

	async function choose(text: string) {
		if (!active || chosen || !onChoose) return;
		// Held while it sends, so a second tap cannot send twice; let go if
		// the chat could not take it, so the buttons still work.
		chosen = text;
		if (!(await onChoose(text))) chosen = null;
	}
</script>

<Choices>
	{#each options as option}
		<Act variant={chosen === option ? "primary" : "quiet"} disabled={!active || !!chosen} onclick={() => choose(option)}>
			{option}
		</Act>
	{/each}
</Choices>
