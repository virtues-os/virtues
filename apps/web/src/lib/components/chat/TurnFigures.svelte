<script lang="ts">
	/**
	 * TurnFigures
	 *
	 * The images a turn's code_interpreter runs saved to out/, shown in the
	 * reply rather than inside the folded "Worked on this" block. A chart is
	 * the answer, not the working, and the model is told the owner can see it.
	 * Only a saved chat's images carry a url; a ghost chat's have none.
	 */
	import { backendUrl } from "$lib/config/backend";

	interface Props {
		/** The turn's parts; only finished code_interpreter calls are read. */
		parts: any[];
	}

	let { parts }: Props = $props();

	// A streamed part is typed `tool-code_interpreter`; a stored one, as a
	// reload reads it back, is `tool-invocation` with the name in toolName.
	const isCodeRun = (p: any) =>
		p.type === "tool-code_interpreter" ||
		(p.type === "tool-invocation" && p.toolName === "code_interpreter");

	const figures = $derived(
		parts
			.filter((p) => isCodeRun(p) && p.state === "output-available")
			.flatMap((p) => (p.output?.images ?? []) as { path: string; url?: string }[])
			.filter((i) => i.url),
	);
</script>

{#if figures.length}
	<div class="turn-figures">
		{#each figures as figure (figure.url)}
			<img src={backendUrl(figure.url!)} alt={figure.path.replace(/^out\//, "")} loading="lazy" />
		{/each}
	</div>
{/if}

<style>
	.turn-figures {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin: 0.5rem 0 0.75rem;
	}

	.turn-figures img {
		display: block;
		max-width: 100%;
		height: auto;
		border-radius: 6px;
		border: 1px solid var(--color-border);
	}
</style>
