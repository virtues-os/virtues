<!--
	ChapterLifelineLive.svelte

	The lifeline plate drawn from the record: the person's own chapters
	(what the interview's close wrote to wiki_chapters) and the birth date
	on their profile. It closes the loop the opening started — the fictional
	plate showed what a partition of a life looks like; this one is theirs.

	Renders nothing until the chapters arrive, and nothing at all when there
	are none (a close whose chapter extraction failed leaves only the
	document; the card below already says so).
-->

<script lang="ts">
	import { getChapters } from "$lib/wiki/api";
	import { getProfile } from "$lib/api/client";
	import ChapterLifeline from "./ChapterLifeline.svelte";
	import { lifeFromRecord, type Life } from "./life";

	let life = $state<Life | null>(null);

	$effect(() => {
		let cancelled = false;
		(async () => {
			// The profile is optional here: no birth date only hides the age
			// ruler. The chapters are the plate.
			const [chapters, profile] = await Promise.all([
				getChapters(),
				getProfile().catch(() => null),
			]);
			if (cancelled) return;
			life = lifeFromRecord(chapters, profile?.birth_date);
		})();
		return () => {
			cancelled = true;
		};
	});
</script>

{#if life}
	<div class="closing-plate">
		<p class="eyebrow">Your chapters</p>
		<ChapterLifeline {life} />
	</div>
{/if}

<style>
	.closing-plate {
		/* Same column as a message; the plate inside bleeds past it. */
		margin: 0.5rem 0 0;
	}

	.eyebrow {
		margin: 0;
		font-size: 0.6875rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--color-foreground-muted);
	}
</style>
