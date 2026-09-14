<!--
	The room's four steps, under the door, drawn as the app's own contents
	minimap (`TableOfContents`) rather than as a fourth invention: a stack of
	short rules, the step you are on dark, the settled ones grey, the ones set
	aside dashed, and each label appearing only under the cursor.

	This is an adapter, not a widget. It turns steps into rows and hands the
	click back to the room, which knows where in the thread each step was
	spoken about. Mirrored, because these hang from the right-hand corner.
-->
<script lang="ts">
	import TableOfContents, { type TocHeading } from "$lib/components/TableOfContents.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	let { onjump }: { onjump?: (stepId: string) => void } = $props();

	const rows = $derived<TocHeading[]>(
		gettingStarted.steps.map((s) => ({
			id: s.id,
			text: s.title,
			state: s.status === "done" ? "done" : s.status === "skipped" ? "skipped" : undefined,
		})),
	);
</script>

{#if !gettingStarted.graduated && rows.length > 0}
	<TableOfContents
		headings={rows}
		activeId={gettingStarted.firstOpen}
		onselect={(id) => onjump?.(id)}
		hideWhenNarrow={false}
		align="right"
	/>
{/if}
