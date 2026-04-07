<!--
	EventTimeline.svelte — Marginalia Timeline

	Single-column narrative layout. Time in the left margin.
	Continuous vertical line with dots at each event.
	Events collapsed by default; the most novel event auto-expands.
	Click event title to toggle expand/collapse.
-->

<script lang="ts">
	import type { DayEvent } from "$lib/wiki/types";
	import {
		getEventDisplayLabel,
		getEventDisplayLocation,
	} from "$lib/wiki/types";

	interface Props {
		events: DayEvent[];
		timezone: string | null;
		hoveredEventId: string | null;
		onhover: (id: string | null) => void;
		pageDate?: Date;
	}

	let { events, timezone, hoveredEventId, onhover, pageDate }: Props = $props();

	const sortedEvents = $derived(
		[...events]
			.filter((e) => !e.userHidden)
			.sort((a, b) => a.startTime.getTime() - b.startTime.getTime()),
	);

	// ── Most novel event (auto-expand) ──────────────────────────────
	const mostNovelId = $derived(() => {
		let best: { id: string; z: number } | null = null;
		for (const e of sortedEvents) {
			if (e.isUnknown) continue;
			const z = e.noveltyZ ?? -Infinity;
			if (z >= 1.0 && (!best || z > best.z)) {
				best = { id: e.id, z };
			}
		}
		return best?.id ?? null;
	});

	// ── Expand/collapse state ───────────────────────────────────────
	let expandedIds = $state<Set<string>>(new Set());
	let initialized = $state(false);

	// Re-initialize when the most novel event changes (new day loaded)
	$effect(() => {
		const novelId = mostNovelId();
		if (novelId) {
			expandedIds = new Set([novelId]);
		} else {
			expandedIds = new Set();
		}
		initialized = true;
	});

	function toggleExpand(id: string) {
		const next = new Set(expandedIds);
		if (next.has(id)) {
			next.delete(id);
		} else {
			next.add(id);
		}
		expandedIds = next;
	}

	const expandableIds = $derived(
		sortedEvents.filter((e) => !e.isUnknown && e.eventSummary).map((e) => e.id),
	);

	const allExpanded = $derived(
		expandableIds.length > 0 && expandableIds.every((id) => expandedIds.has(id)),
	);

	function toggleAll() {
		if (allExpanded) {
			expandedIds = new Set();
		} else {
			expandedIds = new Set(expandableIds);
		}
	}

	// Expose for parent binding
	export { toggleAll, allExpanded };

	// ── Formatting ──────────────────────────────────────────────────

	function formatTime(date: Date): string {
		return date.toLocaleTimeString("en-US", {
			hour: "numeric",
			minute: "2-digit",
			hour12: true,
			timeZone: timezone ?? undefined,
		});
	}

	function formatDuration(minutes: number): string {
		if (minutes < 60) return `${minutes}m`;
		const h = Math.floor(minutes / 60);
		const m = minutes % 60;
		return m > 0 ? `${h}h ${m}m` : `${h}h`;
	}

	function isMostNovel(event: DayEvent): boolean {
		return event.id === mostNovelId();
	}
</script>

<div class="marginalia-timeline">
	{#each sortedEvents as event}
		{@const isUnknown = event.isUnknown ?? false}
		{@const isHovered = hoveredEventId === event.id}
		{@const isNovel = isMostNovel(event)}
		{@const isExpanded = expandedIds.has(event.id)}
		{@const location = getEventDisplayLocation(event)}
		{@const hasDetail = !!event.eventSummary}

		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div
			class="entry"
			class:unknown={isUnknown}
			class:hovered={isHovered}
			onmouseenter={() => onhover(event.id)}
			onmouseleave={() => onhover(null)}
			role="listitem"
		>
			<!-- Time margin -->
			<span class="time">{formatTime(event.startTime)}</span>

			<!-- Dot on the vertical line -->
			<div class="dot-col">
				<div class="dot" class:unknown={isUnknown} class:novel={isNovel}></div>
			</div>

			<!-- Content -->
			<div class="content">
				{#if isUnknown}
					<span class="unknown-text">
						{formatDuration(event.durationMinutes)} · insufficient data
					</span>
				{:else}
					<div class="event-row">
						{#if hasDetail}
							<button
								class="event-header clickable"
								type="button"
								onclick={() => toggleExpand(event.id)}
							>
								<span class="event-label">{getEventDisplayLabel(event)}</span>
								<span class="meta-inline">
									{formatDuration(event.durationMinutes)}{#if location} · {location}{/if}
								</span>
							</button>
						{:else}
							<div class="event-header">
								<span class="event-label">{getEventDisplayLabel(event)}</span>
								<span class="meta-inline">
									{formatDuration(event.durationMinutes)}{#if location} · {location}{/if}
								</span>
							</div>
						{/if}
						{#if isNovel}
							<span class="novelty-badge">Novel</span>
						{/if}
					</div>

					<div class="accordion-body" class:open={isExpanded && !!event.eventSummary}>
						<div class="accordion-inner">
							<p class="event-summary">{event.eventSummary ?? ''}</p>
							<div class="event-meta">
								<span class="meta-text">
									{formatTime(event.startTime)} – {formatTime(event.endTime)}{#if event.topics.length > 0} · {event.topics.join(', ')}{/if}
								</span>
							</div>
						</div>
					</div>
				{/if}
			</div>
		</div>
	{/each}
</div>

<style>
	.marginalia-timeline {
		display: flex;
		flex-direction: column;
		position: relative;
	}

	/* Continuous vertical line running through the dot column */
	.marginalia-timeline::before {
		content: "";
		position: absolute;
		top: 0.5rem;
		bottom: 0.5rem;
		/* 4.25rem (time col) + half of 1.25rem (dot col) = 4.875rem */
		left: 4.875rem;
		width: 1px;
		background: var(--color-border);
		pointer-events: none;
	}

	/* ── Entry grid ────────────────────────────────────────────── */

	.entry {
		display: grid;
		grid-template-columns: 4.25rem 1.25rem 1fr;
		gap: 0;
		padding: 0.75rem 0;
		transition: background 0.12s ease;
		border-radius: 4px;
	}



	/* ── Time margin ───────────────────────────────────────────── */

	.time {
		font-family: var(--font-sans, system-ui, sans-serif);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		text-align: right;
		padding-right: 0.625rem;
		padding-top: 0.15rem;
		line-height: 1.4;
	}


	.entry.unknown .time {
		opacity: 0.5;
	}

	/* ── Dot column (continuous vertical line with dots) ────────── */

	.dot-col {
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding-top: 0.35rem;
		position: relative;
	}


	.dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
		border: 1.5px solid var(--color-surface, white);
		position: relative;
		z-index: 1;
		flex-shrink: 0;
	}

	.dot.unknown {
		width: 5px;
		height: 5px;
		background: var(--color-border);
	}

	.entry.hovered .dot {
		background: var(--color-primary);
	}

	/* ── Content area ──────────────────────────────────────────── */

	.content {
		padding-left: 0.625rem;
		min-width: 0;
	}

	.unknown-text {
		font-size: 0.8125rem;
		line-height: 1.4;
		color: var(--color-foreground-subtle);
		font-style: italic;
	}

	/* ── Event row (header + badge) ───────────────────────────── */

	.event-row {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.5rem;
	}

	/* ── Event header ──────────────────────────────────────────── */

	.event-header {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		flex-wrap: wrap;
		padding: 0.25rem 0.5rem;
		margin: -0.25rem -0.5rem;
		border-radius: 6px;
		transition: background 0.12s ease;
	}

	button.event-header.clickable {
		background: none;
		border: none;
		font: inherit;
		cursor: pointer;
		text-align: left;
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		flex-wrap: wrap;
		padding: 0.25rem 0.5rem;
		margin: -0.25rem -0.5rem;
		border-radius: 6px;
		transition: background 0.12s ease;
		width: fit-content;
	}

	button.event-header.clickable:hover {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.event-label {
		font-family: var(--font-sans, system-ui, sans-serif);
		font-size: 0.9375rem;
		font-weight: 500;
		color: var(--color-foreground);
		line-height: 1.4;
	}

	.meta-inline {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		line-height: 1.4;
	}

	.dot.novel {
		background: var(--color-primary);
		width: 8px;
		height: 8px;
	}

	.novelty-badge {
		font-size: 0.625rem;
		font-weight: 500;
		color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
		padding: 0.0625rem 0.5rem;
		border-radius: 9999px;
	}

	/* ── Accordion (slide open/close) ──────────────────────────── */

	.accordion-body {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 0.25s ease;
	}

	.accordion-body.open {
		grid-template-rows: 1fr;
	}

	.accordion-inner {
		overflow: hidden;
	}

	/* ── Event summary (expanded only) ─────────────────────────── */

	.event-summary {
		font-size: 0.875rem;
		line-height: 1.65;
		color: var(--color-foreground-muted);
		margin: 0.25rem 0 0;
	}

	/* ── Event meta (shown when expanded) ──────────────────────── */

	.event-meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.3rem;
		margin-top: 0.3rem;
	}

	.meta-text {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	/* ── Responsive ────────────────────────────────────────────── */

	@media (max-width: 500px) {
		.entry {
			grid-template-columns: 3.25rem 1rem 1fr;
		}

		.time {
			font-size: 0.625rem;
			padding-right: 0.375rem;
		}
	}
</style>
