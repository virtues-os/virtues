<script lang="ts">
	import { slide } from "svelte/transition";
	import type { DayEvent } from "$lib/wiki/types";
	import {
		getEventDisplayLabel,
		getEventDisplayLocation,
	} from "$lib/wiki/types";

	interface Props {
		events: DayEvent[];
	}

	let { events }: Props = $props();

	let hoveredEventId = $state<string | null>(null);
	let expandedEventId = $state<string | null>(null);

	const hours = [6, 12, 18];

	// Events sorted by start time
	const sortedEvents = $derived(
		[...events].sort(
			(a, b) => a.startTime.getTime() - b.startTime.getTime(),
		),
	);

	// Day boundaries: always midnight-to-midnight (matches hour label positions)
	const dayBoundaries = $derived(() => {
		const ref = sortedEvents.length > 0 ? sortedEvents[0].startTime : new Date();
		const start = new Date(ref);
		start.setUTCHours(0, 0, 0, 0);
		const end = new Date(start);
		end.setUTCDate(end.getUTCDate() + 1);
		return { start, end };
	});

	function getEventStyle(event: DayEvent): { left: string; width: string } {
		const { start: dayStart, end: dayEnd } = dayBoundaries();
		const dayDurationMs = dayEnd.getTime() - dayStart.getTime();

		const eventStartMs = event.startTime.getTime() - dayStart.getTime();
		const eventEndMs = event.endTime.getTime() - dayStart.getTime();

		const leftPct = (eventStartMs / dayDurationMs) * 100;
		const widthPct = Math.max(
			((eventEndMs - eventStartMs) / dayDurationMs) * 100 - 0.1,
			0.15,
		);

		return { left: `${leftPct}%`, width: `${widthPct}%` };
	}

	function formatTime(date: Date): string {
		return date.toLocaleTimeString("en-US", {
			hour: "2-digit",
			minute: "2-digit",
			hour12: false,
		});
	}

	function formatDuration(minutes: number): string {
		if (minutes < 60) return `${minutes}m`;
		const h = Math.floor(minutes / 60);
		const m = minutes % 60;
		return m > 0 ? `${h}h ${m}m` : `${h}h`;
	}

	function formatOntologyId(sourceId: string): string {
		return sourceId
			.split("_")
			.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
			.join(" ");
	}

	function toggleExpand(eventId: string) {
		expandedEventId = expandedEventId === eventId ? null : eventId;
	}
</script>

<div class="day-timeline">
	<!-- Bar visualization -->
	<div class="timeline-bar">
		<div class="bar-track">
			{#each sortedEvents as event}
				{@const style = getEventStyle(event)}
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div
					class="bar-segment"
					class:unknown={event.isUnknown}
					class:transit={event.isTransit}
					class:sleep={event.autoLabel === "Sleep"}
					class:hovered={hoveredEventId === event.id}
					style="left: {style.left}; width: {style.width}"
					onmouseenter={() => (hoveredEventId = event.id)}
					onmouseleave={() => (hoveredEventId = null)}
					title="{getEventDisplayLabel(event)} ({formatTime(
						event.startTime,
					)} - {formatTime(event.endTime)})"
					role="presentation"
				></div>
			{/each}
		</div>
		<div class="bar-labels">
			{#each hours as hour}
				<span class="bar-label" style="left: {(hour / 24) * 100}%">
					{hour.toString().padStart(2, "0")}:00
				</span>
			{/each}
		</div>
	</div>

	<!-- Read-only table -->
	<div class="table-wrapper">
		<table class="timeline-table">
			<thead>
				<tr>
					<th class="col-time">Time</th>
					<th>Event</th>
					<th>Location</th>
					<th class="col-duration">Duration</th>
					<th class="col-ontologies">Ontologies</th>
				</tr>
			</thead>
			<tbody>
				{#each sortedEvents as event}
					{@const isExpanded = expandedEventId === event.id}
					{@const ontologyCount = event.sourceIds?.length || 0}
					<tr
						class:unknown={event.isUnknown}
						class:transit={event.isTransit}
						class:sleep={event.autoLabel === "Sleep"}
						class:hovered={hoveredEventId === event.id}
						class:expanded={isExpanded}
						onmouseenter={() => (hoveredEventId = event.id)}
						onmouseleave={() => (hoveredEventId = null)}
					>
						<td class="cell-time">
							{formatTime(event.startTime)} – {formatTime(
								event.endTime,
							)}
						</td>
						<td class="cell-label">
							{getEventDisplayLabel(event)}
							{#if event.isUserEdited}
								<span
									class="user-edited-indicator"
									title="User edited">✎</span
								>
							{/if}
						</td>
						<td class="cell-location">
							{getEventDisplayLocation(event) || "—"}
						</td>
						<td class="cell-duration">
							{formatDuration(event.durationMinutes)}
						</td>
						<td class="cell-ontologies">
							{#if ontologyCount > 0}
								<button
									class="ontologies-trigger"
									class:active={isExpanded}
									onclick={() => toggleExpand(event.id)}
									type="button"
									aria-expanded={isExpanded}
								>
									<span class="ontologies-count"
										>{ontologyCount}</span
									>
									<svg
										class="chevron"
										width="12"
										height="12"
										viewBox="0 0 12 12"
										fill="none"
									>
										<path
											d="M3 4.5L6 7.5L9 4.5"
											stroke="currentColor"
											stroke-width="1.5"
											stroke-linecap="round"
											stroke-linejoin="round"
										/>
									</svg>
								</button>
							{:else}
								<span class="ontologies-empty">—</span>
							{/if}
						</td>
					</tr>
					{#if isExpanded && ontologyCount > 0}
						<tr class="expandable-row">
							<td colspan="5" class="expandable-cell">
								<div
									class="ontologies-panel"
									transition:slide={{ duration: 200 }}
								>
									<div class="ontologies-list">
										{#each event.sourceIds as sourceId}
											<span class="ontology-tag"
												>{formatOntologyId(
													sourceId,
												)}</span
											>
										{/each}
									</div>
								</div>
							</td>
						</tr>
					{/if}
				{/each}
			</tbody>
		</table>
	</div>
</div>

<style>
	.day-timeline {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	/* Bar */
	.timeline-bar {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}

	.bar-track {
		position: relative;
		height: 32px;
		background-color: var(--color-surface);
		background-image: radial-gradient(
			circle,
			color-mix(in srgb, var(--color-foreground) 25%, transparent) 1px,
			transparent 1px
		);
		background-size: 6px 6px;
		background-position: 2px 2px;
		border-radius: 2px;
	}

	.bar-segment {
		position: absolute;
		top: 2px;
		bottom: 2px;
		background: var(--color-foreground-muted);
		border-radius: 2px;
		transition: all 0.1s ease;
		z-index: 1;
	}

	.bar-segment:hover,
	.bar-segment.hovered {
		background: var(--color-foreground);
	}

	.bar-segment.unknown {
		background: transparent;
		border: 1px dashed var(--color-border);
		z-index: 0;
	}

	.bar-segment.unknown:hover,
	.bar-segment.unknown.hovered {
		background: color-mix(in srgb, var(--color-surface) 50%, transparent);
		border-color: var(--color-foreground-subtle);
	}

	.bar-segment.transit {
		background: var(--color-foreground-subtle);
	}

	.bar-segment.sleep {
		background-color: var(--color-surface);
		background-image: repeating-linear-gradient(
			-45deg,
			var(--color-foreground-subtle),
			var(--color-foreground-subtle) 1px,
			transparent 1px,
			transparent 4px
		);
		border: 1px solid var(--color-foreground-subtle);
	}

	.bar-segment.sleep:hover,
	.bar-segment.sleep.hovered {
		background-color: var(--color-surface);
		background-image: repeating-linear-gradient(
			-45deg,
			var(--color-foreground-muted),
			var(--color-foreground-muted) 1px,
			transparent 1px,
			transparent 4px
		);
		border-color: var(--color-foreground-muted);
	}

	.bar-labels {
		position: relative;
		height: 1rem;
		margin: 0 1rem;
	}

	.bar-label {
		position: absolute;
		transform: translateX(-50%);
		font-size: 0.625rem;
		color: var(--color-foreground-subtle);
	}

	/* Table */
	.table-wrapper {
		border: 1px solid var(--color-border);
		border-radius: 4px;
		overflow: hidden;
	}

	.timeline-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
	}

	.timeline-table thead {
		background: var(--color-surface-elevated);
	}

	.timeline-table th {
		text-align: left;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.75rem;
		font-weight: 400;
		color: var(--color-foreground-muted);
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
	}

	.timeline-table td {
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		vertical-align: middle;
	}

	.timeline-table tbody tr:last-child:not(.expandable-row) td {
		border-bottom: none;
	}

	.timeline-table tbody tr {
		transition: background 0.1s ease;
	}

	.timeline-table tbody tr:hover,
	.timeline-table tbody tr.hovered {
		background: color-mix(in srgb, var(--color-primary) 5%, transparent);
	}

	.timeline-table tr.unknown {
		color: var(--color-foreground-muted);
	}

	.timeline-table tr.transit {
		color: var(--color-foreground-muted);
	}

	.timeline-table tr.sleep {
		color: var(--color-foreground-muted);
	}

	.col-time {
		width: 120px;
	}

	.col-duration {
		width: 70px;
		text-align: right;
	}

	.col-ontologies {
		width: 48px;
	}

	.cell-time {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	.cell-label {
		color: var(--color-foreground);
	}

	.user-edited-indicator {
		margin-left: 0.375rem;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		opacity: 0.6;
	}

	.cell-location {
		color: var(--color-foreground-muted);
	}

	.cell-duration {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		text-align: right;
	}

	.cell-ontologies {
		text-align: right;
		padding-right: 0.5rem !important;
	}

	.ontologies-trigger {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		background: none;
		border: none;
		padding: 0.25rem 0.375rem;
		cursor: pointer;
		color: var(--color-foreground-muted);
		font-size: 0.75rem;
		border-radius: 3px;
		transition: all 0.15s ease;
	}

	.ontologies-trigger:hover {
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}

	.ontologies-count {
		font-weight: 500;
	}

	.chevron {
		transition: transform 0.2s ease;
	}

	.ontologies-trigger.active .chevron {
		transform: rotate(180deg);
	}

	.ontologies-empty {
		color: var(--color-foreground-subtle);
		font-size: 0.75rem;
	}

	.timeline-table tbody tr.expanded td {
		border-bottom: none;
	}

	.expandable-row {
		background: var(--color-surface-elevated);
	}

	.expandable-cell {
		padding: 0 !important;
	}

	.ontologies-panel {
		padding: 0.75rem;
		border-top: 1px solid var(--color-border);
		overflow: hidden;
	}

	.ontologies-list {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
	}

	.ontology-tag {
		display: inline-block;
		padding: 0.25rem 0.5rem;
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
		color: var(--color-foreground);
		border-radius: 3px;
		font-size: 0.6875rem;
		font-family: var(--font-mono, monospace);
	}
</style>
