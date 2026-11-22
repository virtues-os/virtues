<script lang="ts">
	/**
	 * The Inbox as today's route. The solid line is where you have been, from
	 * the day's recorded path and stops. The dotted line is the rest of the
	 * plan, for the calendar entries your server can place: at a place you
	 * stopped at today, or at one of your places that the entry names, which
	 * this looks up. The route lies on a street map from your server's own
	 * map files when it has them (RouteGround), and on plain paper when it
	 * does not. A thing in the Inbox with a time today is a dotted mark on
	 * the route where that time falls, with its time and name beside it.
	 * Under the route is the day's clock, with the same stops and entries by
	 * time and each thing at its own time. A thing with no time of its own
	 * today sits in a row under the clock. The geometry is in
	 * lib/inbox/route.
	 */
	import { untrack } from 'svelte';
	import { TextAction } from '$lib';
	import { fetchDay, type TimelineDay } from '$lib/timeline/day';
	import { getPlaceById, listPlaces } from '$lib/wiki/api';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import { MARK_SIZE, type InboxItem } from '$lib/inbox/items';
	import { fitLabels, markBox, type Box, type Wanted } from '$lib/inbox/labels';
	import {
		CLOCK_X0,
		CLOCK_X1,
		MAP_H,
		MAP_W,
		buildRoute,
		clockScale,
		dayBars,
		dayBounds,
		entriesAhead,
		geoBox,
		itemTime,
		stack,
		type NamedPlace,
		type Project,
		type Pt,
	} from '$lib/inbox/route';
	import FitBox from './FitBox.svelte';
	import InboxDot from './InboxDot.svelte';
	import RouteGround from './RouteGround.svelte';

	let {
		items,
		now,
		picked = null,
		faded,
		onPick,
	}: {
		/** The things for now and today. */
		items: InboxItem[];
		now: Date;
		picked?: string | null;
		faded?: (it: InboxItem) => boolean;
		onPick: (key: string) => void;
	} = $props();

	// The clock's rows, top to bottom, in the drawing's own pixels.
	const HOURS_Y = MAP_H + 8;
	const BARS_Y = MAP_H + 32;
	const LINE_Y = MAP_H + 72;
	const MARKS_Y = MAP_H + 96;
	const TRAY_Y = MAP_H + 152;
	const H = MAP_H + 192;

	const slug = $derived(getLocalDateSlug(now));
	/** The day is read again every five minutes, because its record grows as the day goes. */
	const turn = $derived(Math.floor(now.getTime() / 300_000));
	let day = $state<TimelineDay | null>(null);
	let failed = $state(false);
	/** Bumped by Try again. */
	let tries = $state(0);

	$effect(() => {
		const s = slug;
		void turn;
		void tries;
		fetchDay(s)
			.then((d) => {
				day = d;
				failed = false;
			})
			.catch((e) => {
				console.error('[InboxMap] day failed:', e);
				failed = true;
			});
	});

	const nowMs = $derived(now.getTime());
	const bounds = $derived(day ? dayBounds(day) : null);

	/**
	 * Your places that an entry still ahead names, with where each is. A
	 * calendar entry carries only a name, and the day's own stops cover only
	 * where you have been, so a place not yet reached today is looked up
	 * among your places. Each location is asked about once.
	 */
	let lookedUp = $state.raw<NamedPlace[]>([]);
	const asked = new Set<string>();
	$effect(() => {
		if (!day) return;
		const names = entriesAhead(day, nowMs)
			.map((e) => e.location_name)
			.filter((name): name is string => !!name && !asked.has(name));
		if (names.length === 0) return;
		for (const name of names) asked.add(name);
		void (async () => {
			const places = await listPlaces();
			const found: NamedPlace[] = [];
			for (const name of names) {
				const match = places.find((p) => p.name.trim().length >= 3 && name.toLowerCase().includes(p.name.trim().toLowerCase()));
				const place = match ? await getPlaceById(match.id) : null;
				if (place && place.latitude !== null && place.longitude !== null) found.push({ name: place.name, lat: place.latitude, lng: place.longitude });
			}
			if (found.length > 0) lookedUp = [...untrack(() => lookedUp), ...found.filter((f) => !untrack(() => lookedUp).some((p) => p.name === f.name))];
		})().catch((e) => console.error('[InboxMap] place lookup failed:', e));
	});

	/** How the map under the route places a point, once it is drawn. Null while there is no map. */
	let onMap = $state.raw<Project | null>(null);
	const ground = $derived(day ? geoBox(day, nowMs, lookedUp) : null);
	const route = $derived(day ? buildRoute(day, nowMs, onMap ?? undefined, lookedUp) : null);
	const bars = $derived(day ? dayBars(day, nowMs) : []);

	const timed = $derived(bounds ? items.map((it) => ({ it, t: itemTime(it, bounds) })) : []);
	const clocked = $derived(timed.filter((x): x is { it: InboxItem; t: number } => x.t !== null));
	const waiting = $derived(timed.filter((x) => x.t === null && x.it.kind === 'needs').map((x) => x.it));
	const anytime = $derived(timed.filter((x) => x.t === null && x.it.kind !== 'needs').map((x) => x.it));

	const scale = $derived(bounds ? clockScale(bounds, [...bars.map((b) => b.from), ...clocked.map((x) => x.t)]) : null);
	const marks = $derived(scale ? stack(clocked.map((x) => ({ key: x.it.key, x: scale.x(x.t) }))) : {});
	const hours = $derived.by(() => {
		if (!scale || !bounds) return [];
		const out: { x: number; name: string }[] = [];
		for (let h = Math.ceil(scale.fromHour / 3) * 3; h < 24; h += 3) {
			out.push({ x: scale.x(bounds.startMs + h * 3_600_000), name: `${h % 12 || 12} ${h < 12 ? 'AM' : 'PM'}` });
		}
		return out;
	});

	/** Things with a time, where that time falls on the route. Ones that share a spot sit side by side. */
	const onRoute = $derived.by(() => {
		if (!route) return [];
		const taken = new Map<string, number>();
		const out: { it: InboxItem; t: number; x: number; y: number }[] = [];
		for (const { it, t } of clocked) {
			const p = route.at(t);
			if (!p) continue;
			const spot = `${Math.round(p.x / 8)}:${Math.round(p.y / 8)}`;
			const n = taken.get(spot) ?? 0;
			taken.set(spot, n + 1);
			out.push({ it, t, x: p.x + n * 24, y: p.y + 24 });
		}
		return out;
	});
	const pin = $derived(onRoute.find((m) => m.it.key === picked) ?? null);

	const zone = $derived(day?.zone);
	const clock = (ms: number) => new Date(ms).toLocaleTimeString('en-US', { timeZone: zone, hour: 'numeric', minute: '2-digit' });
	const today = $derived(now.toLocaleDateString('en-US', { weekday: 'long', month: 'long', day: 'numeric' }));
	const points = (line: Pt[]) => line.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(' ');
	const hasRoute = $derived(!!route?.you);
	const clip = (s: string) => (s.length > 30 ? `${s.slice(0, 28).trimEnd()}…` : s);

	/** The caption's room at the top left, so no name lands under it. */
	const CAPTION: Box = { x1: 12, y1: 12, x2: 548, y2: 72 };
	type Tag = Wanted & { tone: '' | 'planned' | 'here' | 'note' };

	/**
	 * The names on the map: where you are, the thing you picked, every stop
	 * with the time you got there, and each thing on the route with its
	 * time. Each is placed clear of the others, of the stops and marks, and
	 * of the caption (lib/inbox/labels), so stops that sit close together
	 * keep readable names. A thing's name is the first to go where there is
	 * no room for it; pointing at its mark still names it.
	 */
	const tags = $derived.by(() => {
		const list: Tag[] = [];
		if (!route) return { list, spots: {} as ReturnType<typeof fitLabels> };
		// A name goes toward the middle of the map first, where there is more room.
		const right = (x: number) => x < MAP_W * 0.62;
		if (route.you) {
			list.push({ key: 'you', text: `${clock(nowMs)} You`, x: route.you.x, y: route.you.y, size: 16, right: right(route.you.x), pinned: true, tone: 'here' });
		}
		if (pin) {
			list.push({ key: 'pin', text: clip(pin.it.title), x: pin.x, y: pin.y, size: MARK_SIZE[pin.it.kind], right: right(pin.x), pinned: true, tone: 'here' });
		}
		for (const s of route.stops) {
			list.push({
				key: s.key,
				// A stop you were already at when the day began has no arrival time in this day.
				text: clip(bounds && s.from <= bounds.startMs ? s.name : `${clock(s.from)} ${s.name}`),
				x: s.x,
				y: s.y,
				size: s.planned ? 14 : 12,
				right: right(s.x),
				pinned: true,
				tone: s.planned ? 'planned' : '',
			});
		}
		for (const m of onRoute) {
			if (m.it.key === picked) continue;
			list.push({ key: `note:${m.it.key}`, text: clip(`${clock(m.t)} ${m.it.title}`), x: m.x, y: m.y, size: MARK_SIZE[m.it.kind], right: right(m.x), pinned: false, tone: 'note' });
		}
		const taken = [CAPTION, ...list.map(markBox), ...onRoute.map((m) => markBox({ x: m.x, y: m.y, size: MARK_SIZE[m.it.kind] }))];
		return { list, spots: fitLabels(list, taken, { w: MAP_W, h: MAP_H }) };
	});
</script>

<FitBox w={MAP_W} h={H} sheet>
	<div class="frame" style:width="{MAP_W}px" style:height="{MAP_H}px">
		{#if ground}<RouteGround box={ground} onProject={(p) => (onMap = p)} />{/if}
	</div>

	<div class="caption">
		<span class="caps">{today}</span>
		<span class="lede">The solid line is where you have been. The dotted line is the rest of the plan.</span>
	</div>

	{#if failed && !day}
		<p class="empty">
			Your server couldn't load today. <TextAction inline onclick={() => (tries += 1)}>Try again</TextAction>
		</p>
	{:else if day && !hasRoute}
		<p class="empty">No location recorded today. When your phone records where you are, your day draws here as a route.</p>
	{/if}

	<svg class="lines" width={MAP_W} height={H} viewBox="0 0 {MAP_W} {H}" aria-hidden="true">
		{#if route}
			{#each route.bridges as line, i (i)}<polyline class="bridge" points={points(line)} />{/each}
			{#each route.runs as line, i (i)}<polyline class="casing" points={points(line)} />{/each}
			{#if route.ahead}<path class="casing" d={route.ahead} />{/if}
			{#each route.runs as line, i (i)}<polyline class="been" points={points(line)} />{/each}
			{#if route.ahead}<path class="ahead" d={route.ahead} />{/if}
			{#each tags.list as t (t.key)}
				{@const lead = tags.spots[t.key]?.lead}
				{#if lead}<line class="lead" x1={lead.x1} y1={lead.y1} x2={lead.x2} y2={lead.y2} />{/if}
			{/each}
		{/if}
		{#if scale}
			{@const nowX = scale.x(nowMs)}
			<line class="been" x1={CLOCK_X0} y1={LINE_Y} x2={nowX} y2={LINE_Y} />
			<line class="ahead" x1={nowX} y1={LINE_Y} x2={CLOCK_X1} y2={LINE_Y} />
			<line class="now" x1={nowX} y1={BARS_Y - 8} x2={nowX} y2={MARKS_Y + 8} />
		{/if}
	</svg>

	{#if route}
		{#each route.stops as s (s.key)}
			<span class="stop" class:planned={s.planned} style:left="{s.x}px" style:top="{s.y}px"></span>
		{/each}
		{#if route.you}
			<span class="you" style:left="{route.you.x}px" style:top="{route.you.y}px"></span>
		{/if}
		{#each onRoute as m (m.it.key)}
			<InboxDot item={m.it} x={m.x} y={m.y} picked={m.it.key === picked} faded={!!faded?.(m.it)} {onPick} />
		{/each}
		{#each tags.list as t (t.key)}
			{@const spot = tags.spots[t.key]}
			{#if spot}
				<span class="tag {t.tone}" class:leftward={!spot.right} style:left="{spot.anchor}px" style:top="{spot.y}px">{t.text}</span>
			{/if}
		{/each}
	{/if}

	{#if scale}
		{#each hours as h (h.name)}
			<span class="hour" style:left="{h.x}px" style:top="{HOURS_Y}px">{h.name}</span>
		{/each}
		{#each bars as b (b.key)}
			{@const x = scale.x(b.from)}
			{@const w = Math.max(scale.x(b.to) - x, 6)}
			<span class="bar" class:planned={b.planned} style:left="{x}px" style:top="{BARS_Y}px" style:width="{w}px" data-tip="{b.label}, {clock(b.from)} to {clock(b.to)}">
				{w > 56 ? b.label : ''}
			</span>
		{/each}
		{#each clocked as { it } (it.key)}
			{@const m = marks[it.key]}
			<InboxDot item={it} x={m.x} y={MARKS_Y + Math.min(m.row, 2) * 22} picked={it.key === picked} faded={!!faded?.(it)} {onPick} />
		{/each}

		<div class="tray" style:top="{TRAY_Y}px">
			{#if waiting.length > 0}
				<span class="group">
					<span class="caps">Waiting on you</span>
					{#each waiting as it (it.key)}
						<InboxDot item={it} picked={it.key === picked} faded={!!faded?.(it)} {onPick} />
					{/each}
				</span>
			{/if}
			{#if anytime.length > 0}
				<span class="group">
					<span class="caps">Today, no set time</span>
					{#each anytime as it (it.key)}
						<InboxDot item={it} picked={it.key === picked} faded={!!faded?.(it)} {onPick} />
					{/each}
				</span>
			{/if}
		</div>
	{/if}
</FitBox>

<style>
	.frame {
		position: absolute;
		left: 0;
		top: 0;
	}
	/* On the page's own paper, so it reads over a map. */
	.caption {
		position: absolute;
		left: 12px;
		top: 12px;
		z-index: 1;
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--color-page) 88%, transparent);
	}
	.caps {
		font-size: var(--text-caption);
		line-height: 16px;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		white-space: nowrap;
		color: var(--color-foreground-muted);
	}
	.lede {
		font-family: var(--font-serif-ui);
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.empty {
		position: absolute;
		left: 50%;
		top: 208px;
		width: 420px;
		margin: 0;
		transform: translateX(-50%);
		font-size: var(--text-label);
		line-height: 24px;
		text-align: center;
		color: var(--color-foreground-muted);
	}

	.lines {
		position: absolute;
		left: 0;
		top: 0;
		pointer-events: none;
	}
	.lines polyline,
	.lines path,
	.lines line {
		fill: none;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
	/* Paper under each line, so it stays clear of the streets it crosses. */
	.casing {
		stroke: var(--color-page);
		stroke-width: 8;
		opacity: 0.75;
	}
	.been {
		stroke: var(--color-foreground);
		stroke-width: 3.5;
	}
	.ahead {
		stroke: var(--color-primary);
		stroke-width: 2.5;
		stroke-dasharray: 2 7;
	}
	/* A stretch nothing recorded, between two recorded runs. */
	.bridge {
		stroke: var(--color-border-field);
		stroke-width: 1.5;
	}
	.now {
		stroke: var(--color-primary);
		stroke-width: 1;
	}
	/* The line from a name back to its stop, when the name had to sit away from it. */
	.lead {
		stroke: var(--color-foreground-muted);
		stroke-width: 1;
	}

	.stop,
	.you {
		position: absolute;
		z-index: 2;
		box-sizing: border-box;
		border-radius: 50%;
		transform: translate(-50%, -50%);
	}
	.stop {
		width: 12px;
		height: 12px;
		border: 2px solid var(--color-page);
		background: var(--color-foreground);
	}
	.stop.planned {
		width: 14px;
		height: 14px;
		border: 2px solid var(--color-primary);
		background: var(--color-page);
	}
	.you {
		width: 16px;
		height: 16px;
		border: 4px solid color-mix(in srgb, var(--color-primary) 24%, var(--color-page));
		background: var(--color-primary);
	}
	.tag {
		position: absolute;
		z-index: 2;
		padding: 0 4px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--color-page) 88%, transparent);
		font-family: var(--font-serif-ui);
		font-size: var(--text-label);
		line-height: 20px;
		white-space: nowrap;
		color: var(--color-foreground);
		pointer-events: none;
	}
	/* A name on the left of its mark ends at its anchor. */
	.tag.leftward {
		transform: translateX(-100%);
	}
	.tag.planned,
	.tag.here {
		color: var(--color-primary);
	}
	/* A thing in the Inbox: not lived yet, so its name wears a dotted line. */
	.tag.note {
		border: 1px dotted var(--color-border-field);
		font-family: var(--font-sans);
		font-size: var(--text-caption);
		line-height: 18px; /* design-ok: 20px less the 1px border, top and bottom */
		color: var(--color-foreground-muted);
	}

	.hour {
		position: absolute;
		transform: translateX(-50%);
		font-size: var(--text-caption);
		line-height: 16px;
		white-space: nowrap;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.bar {
		position: absolute;
		box-sizing: border-box;
		height: 24px;
		padding: 0 8px;
		border-radius: var(--radius-sm);
		background: var(--active-bg);
		overflow: hidden;
		font-size: var(--text-caption);
		line-height: 24px;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-foreground);
	}
	.bar.planned {
		border: 1px dashed var(--color-primary);
		background: var(--color-page);
		line-height: 22px;
		color: var(--color-primary);
	}
	.tray {
		position: absolute;
		left: 24px;
		right: 24px;
		display: flex;
		flex-wrap: wrap;
		gap: 8px 32px;
	}
	.group {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px;
	}
</style>
