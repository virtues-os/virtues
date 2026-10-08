<!--
	DayPlaces.svelte - where the phone was on a day: the stops on a strip
	across the day's clock, and the track and stops on a map. Hovering the
	strip moves a dot along the track.

	A stop nobody named carries its coordinates as a name; here it reads
	"Unnamed place", on the strip and on the map alike.
-->

<script lang="ts">
	import { getLocalDateSlug } from "$lib/utils/dateUtils";
	import type { TimelineDayLocationChunk } from "$lib/wiki/api";
	import { placeLabel } from "$lib/wiki/placeName";
	import MovementMap from "$lib/components/timeline/MovementMap.svelte";
	import DayLocationTimeline from "$lib/components/timeline/DayLocationTimeline.svelte";

	interface Props {
		movementStops: TimelineDayLocationChunk[];
		movementTrack: { lat: number; lng: number; timeMs: number }[];
		dedupedMarkers: { lat: number; lng: number; label: string; timeMs: number; placeId: string | null }[];
		hasLocationData: boolean;
		timezone: string | null;
		pageDate: Date;
	}

	let { movementStops, movementTrack, dedupedMarkers, hasLocationData, timezone, pageDate }: Props = $props();

	const dayDate = $derived(getLocalDateSlug(pageDate));
	const stops = $derived(movementStops.map((s) => ({ ...s, place_name: placeLabel(s.place_name) })));
	const markers = $derived(dedupedMarkers.map((m) => ({ ...m, label: placeLabel(m.label) })));

	let hoverTimeMs = $state<number | null>(null);
</script>

{#if hasLocationData}
	<div class="places">
		{#if stops.length > 0}
			<DayLocationTimeline visits={stops} {dayDate} {timezone} bind:hoverTimeMs />
		{/if}
		<MovementMap track={movementTrack} stops={markers} height={260} {hoverTimeMs} />
	</div>
{:else}
	<p class="note">No location recorded this day</p>
{/if}

<style>
	.places {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		margin-top: 0.75rem;
	}

	.note {
		margin: 0.5rem 0 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}
</style>
