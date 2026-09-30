<!--
	TimelineView.svelte — our Timeline instrument, map mode.

	A MapLibre map on the box's own atlas tiles (Protomaps, served from
	/api/map/vt) showing one local day's GPS path: solid runs of recorded fixes
	and dashed bridges across the holes in the recording ($lib/timeline/track).
	A bar at the top steps between days. Our resolved stays follow.

	Our own map (MapLibre v6), not the Leaflet MovementMap: the prototype's map
	logic is MapLibre-native, and this is where it gets ported.
-->
<script lang="ts">
	import { onMount, onDestroy, tick } from 'svelte';
	import { browser } from '$app/environment';
	import { atlasStyle } from '$lib/map/atlas';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import type { GeoJSONSource, Map as MlMap, StyleSpecification } from 'maplibre-gl';
	import { fetchDayBounds, fetchDayWindow, fetchDerived, lastMeasured, localDay, quietDayVerdict, stepDay } from '$lib/timeline/day';
	import { cleanTrack, dropSpikes, flagHoles, splitTrack, toFixes, type Fix, type Line } from '$lib/timeline/track';
	import { buildRail, type RailPick, type RailSection } from '$lib/timeline/rail';
	import { anchorAt } from '$lib/timeline/anchor';
	import TimelineRail from '$lib/components/timeline/TimelineRail.svelte';
	import { APPLE_LIGHT, hsl, recolour } from '$lib/timeline/palette';
	import { COLOURS, colourVars } from '$lib/timeline/colours';

	// The path and the pin are location, so they wear the place colour.
	const TRACK = COLOURS.place;
	// Without the box's map archives there is no basemap: a plain surface in the
	// palette's land colour, so the path still draws and the page doesn't read
	// as broken.
	const BARE: StyleSpecification = {
		version: 8,
		sources: {},
		layers: [{ id: 'bare', type: 'background', paint: { 'background-color': hsl(APPLE_LIGHT.land) } }],
	};

	let container: HTMLDivElement;
	let dayBar = $state<HTMLDivElement | null>(null);
	let picker = $state<HTMLInputElement | null>(null);
	let note = $state<{ title: string; lines: string[] } | null>(null);
	let map: MlMap | null = null;
	let maplibre: typeof import('maplibre-gl') | null = null;
	let resizer: ResizeObserver | null = null;
	let rail = $state<ReturnType<typeof TimelineRail> | null>(null);
	let sections = $state<RailSection[]>([]);
	let zone = $state('UTC');
	let picked = $state<RailPick | null>(null);
	let railError = $state(false);
	/** The day's cleaned track: where a picked moment is found on the map. */
	let dayTrack: Fix[] = [];

	const today = getLocalDateSlug();
	let date = $state(today);
	let ready = $state(false);
	let basemap = $state(true);
	let status = $state<'loading' | 'shown' | 'empty' | 'error'>('loading');
	let asked = 0; // the latest day asked for, so a slow answer can't overwrite a newer one

	const label = $derived.by(() => {
		const d = new Date(localDay(date).startMs);
		const thisYear = d.getFullYear() === new Date().getFullYear();
		return d.toLocaleDateString(undefined, {
			weekday: 'short',
			month: 'short',
			day: 'numeric',
			...(thisYear ? {} : { year: 'numeric' }),
		});
	});

	const lines = (coordinates: Line[]): GeoJSON.Feature<GeoJSON.MultiLineString> => ({
		type: 'Feature',
		properties: {},
		geometry: { type: 'MultiLineString', coordinates },
	});

	onMount(() => {
		if (!browser) return;
		let cancelled = false;
		(async () => {
			// MapLibre's worker sits where the bundler put it, not next to the
			// module, so set it explicitly — the same way $lib/map/atlas does.
			const [ml, workerUrl] = await Promise.all([
				import('maplibre-gl'),
				import('maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url').then((m) => m.default),
				import('maplibre-gl/dist/maplibre-gl.css'),
			]);
			ml.setWorkerUrl(workerUrl as string);
			if (cancelled || !container) return;
			const style = await atlasStyle('light');
			if (cancelled || !container) return;
			basemap = style !== null;
			const m = new ml.Map({
				container,
				style: style ? recolour(style, APPLE_LIGHT) : BARE,
				center: [0, 20],
				zoom: 1.5,
				// v6: attributionControl takes options (or false), not a bare boolean.
				attributionControl: {},
			});
			map = m;
			maplibre = ml;
			// MapLibre only follows the window's size. A hidden tab stays mounted
			// and split view narrows it, so follow the container instead.
			resizer = new ResizeObserver(() => m.resize());
			resizer.observe(container);
			m.on('load', () => {
				if (cancelled) return;
				m.addSource('track-run', { type: 'geojson', data: lines([]) });
				m.addSource('track-bridge', { type: 'geojson', data: lines([]) });
				m.addLayer({
					id: 'track-run',
					type: 'line',
					source: 'track-run',
					layout: { 'line-join': 'round', 'line-cap': 'round' },
					paint: { 'line-color': TRACK, 'line-width': 3, 'line-opacity': 0.95 },
				});
				// A hole in the recording: thin and dashed, so the straight hop
				// reads as a bridge, not a route.
				m.addLayer({
					id: 'track-bridge',
					type: 'line',
					source: 'track-bridge',
					layout: { 'line-join': 'round', 'line-cap': 'butt' },
					paint: { 'line-color': TRACK, 'line-width': 1.5, 'line-opacity': 0.7, 'line-dasharray': [2, 3] },
				});
				ready = true;
			});
		})();
		return () => {
			cancelled = true;
		};
	});

	$effect(() => {
		if (ready) void showDay(date);
	});

	async function showDay(slug: string) {
		const m = map;
		const ml = maplibre;
		if (!m || !ml) return;
		const mine = ++asked;
		status = 'loading';
		note = null;
		picked = null;
		let bounds, window, derived;
		try {
			// The day in the zone it woke up in, then every fix around it and the
			// server's stays, drives and moments over it.
			bounds = await fetchDayBounds(slug);
			[window, derived] = await Promise.all([
				fetchDayWindow(bounds.startMs, bounds.endMs),
				fetchDerived(bounds).catch((e) => {
					console.warn('[Timeline] derived fetch failed', e);
					return null;
				}),
			]);
		} catch (e) {
			console.warn('[Timeline] day fetch failed', e);
			if (mine === asked) status = 'error';
			return;
		}
		if (mine !== asked) return;
		const { startMs, endMs } = bounds;
		zone = bounds.zone;
		railError = derived === null;
		sections = derived?.is_built ? buildRail(derived, startMs, endMs) : [];
		// Spikes go first, then holes are judged across the whole window, then the
		// day is cut out of it.
		const all = flagHoles(dropSpikes(toFixes(window.points)));
		const fixes = all.filter((f) => f.t >= startMs && f.t < endMs);
		dayTrack = cleanTrack(fixes);
		const { runs, bridges } = splitTrack(dayTrack);
		(m.getSource('track-run') as GeoJSONSource).setData(lines(runs));
		(m.getSource('track-bridge') as GeoJSONSource).setData(lines(bridges));
		// The rail draws first, so the framing can leave room for it.
		await tick();
		if (mine !== asked) return;
		if (fixes.length) {
			status = 'shown';
			const bounds = new ml.LngLatBounds();
			for (const f of fixes) bounds.extend([f.lng, f.lat]);
			m.fitBounds(bounds, { padding: fitPad(m), maxZoom: 15, duration: 0 });
			return;
		}
		// No fix all day. The prototype's rule (`honestWhere` + `gapWhy`): hold the
		// last position, say when it was measured and what the phone did meanwhile.
		status = 'empty';
		const last = all.filter((f) => f.t < startMs).at(-1) ?? toFixes(window.before ? [window.before] : [])[0] ?? null;
		// The prototype's follow zoom is 14 (dayback/src/main.js:938).
		if (last) m.jumpTo({ center: [last.lng, last.lat], zoom: 14 });
		else m.jumpTo({ center: [0, 20], zoom: 1.5 });
		const measured = last ? lastMeasured(last.t, false, zone) : null;
		note = { title: 'No location fix', lines: measured ? [sentence(measured)] : [] };
		let verdict: string | null = null;
		try {
			verdict = await quietDayVerdict(slug);
		} catch (e) {
			console.warn('[Timeline] day verdict failed', e);
		}
		if (mine !== asked || !verdict || !note) return;
		note = { ...note, lines: [...note.lines, verdict] };
	}

	const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

	/** The prototype's framing padding (`v4FitPad`, dayback/src/main.js:1190):
	 *  the measured top bar and rail plus a buffer of max(34 px, 6 % of the
	 *  smaller side) on every edge, each capped so fitBounds can still move.
	 *  The scrubber, once built, reserves its space here too. */
	function fitPad(m: MlMap) {
		const c = m.getContainer();
		const cw = c.clientWidth || 900;
		const ch = c.clientHeight || 600;
		const buf = Math.max(34, Math.round(Math.min(cw, ch) * 0.06));
		const bar = dayBar?.getBoundingClientRect();
		const top = bar ? bar.bottom - c.getBoundingClientRect().top : 58;
		const railW = rail?.width() ?? 0;
		return {
			top: Math.min(top + buf, ch * 0.34),
			bottom: Math.min(buf, ch * 0.45),
			left: Math.min(buf, cw * 0.4),
			right: Math.min((railW ? railW + 16 : 0) + buf, cw * 0.5),
		};
	}

	/** A rail pick takes the map there (`v4RevealSection`, main.js:1095-1101):
	 *  a drive is framed whole; anything else pans to where the track says you
	 *  were at its midpoint, and only if that spot is off the clear map. */
	function reveal(p: RailPick) {
		picked = p;
		const m = map;
		const ml = maplibre;
		if (!m || !ml) return;
		if (p.kind === 'transit') {
			const path = dayTrack.filter((f) => f.t >= p.s && f.t < p.e);
			if (path.length >= 2) {
				const b = new ml.LngLatBounds();
				for (const f of path) b.extend([f.lng, f.lat]);
				m.fitBounds(b, { padding: fitPad(m), maxZoom: 15, duration: 650 });
				return;
			}
		}
		const at = anchorAt(dayTrack, p.s, p.e);
		if (at) showPoint(m, [at.lng, at.lat]);
	}

	/** Bring one spot onto the clear map (`v4ShowPoint`, main.js:1092-1094):
	 *  nothing if it is already there, else ease it to the centre of the clear
	 *  area at the same zoom - a pan, never a re-frame. */
	function showPoint(m: MlMap, ll: [number, number]) {
		const pad = fitPad(m);
		const c = m.getContainer();
		const lim = { l: pad.left, t: pad.top, r: c.clientWidth - pad.right, b: c.clientHeight - pad.bottom };
		const p = m.project(ll);
		if (p.x > lim.l + 40 && p.x < lim.r - 40 && p.y > lim.t + 40 && p.y < lim.b - 40) return;
		const offset: [number, number] = [(lim.l + lim.r) / 2 - c.clientWidth / 2, (lim.t + lim.b) / 2 - c.clientHeight / 2];
		m.easeTo({ center: ll, offset, duration: 650 });
	}

	onDestroy(() => {
		resizer?.disconnect();
		resizer = null;
		map?.remove();
		map = null;
	});
</script>

<div class="timeline" style={colourVars}>
	<div class="timeline-map" bind:this={container}></div>

	<div class="day-bar tile" bind:this={dayBar}>
		<button class="step" aria-label="Previous day" onclick={() => (date = stepDay(date, -1))}>‹</button>
		<button class="date" onclick={() => picker?.showPicker?.()}>{label}</button>
		<button class="step" aria-label="Next day" disabled={date >= today} onclick={() => (date = stepDay(date, 1))}>›</button>
		{#if date !== today}
			<button class="today" onclick={() => (date = today)}>Today</button>
		{/if}
		<input
			bind:this={picker}
			class="picker"
			type="date"
			max={today}
			value={date}
			tabindex="-1"
			aria-hidden="true"
			onchange={(e) => {
				if (e.currentTarget.value) date = e.currentTarget.value;
			}}
		/>
	</div>

	<TimelineRail bind:this={rail} {sections} {zone} {picked} onpick={reveal} />
	{#if railError && status !== 'error' && status !== 'loading'}
		<p class="rail-error tile">Your server couldn't load the day's stays. Reload the page to try again.</p>
	{/if}

	{#if status === 'empty' && note}
		<div class="note tile">
			<p class="note-title">{note.title}</p>
			{#each note.lines as line (line)}
				<p class="note-line">{line}</p>
			{/each}
		</div>
	{:else if status === 'error'}
		<div class="note tile"><p class="note-title">Your server couldn't load {label}. Reload the page to try again.</p></div>
	{/if}
	{#if !basemap}
		<p class="no-basemap">Your server has no map tiles yet</p>
	{/if}
</div>

<style>
	.timeline {
		position: absolute;
		inset: 0;
	}
	.timeline-map {
		position: absolute;
		inset: 0;
	}
	.day-bar {
		position: absolute;
		top: 16px;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		gap: 2px;
		padding: 4px;
		border-radius: 999px;
	}
	/* A tile over the map, in the prototype's look: a solid ground, a faint
	   ink border and one soft shadow (dayback/index.html:599-601). */
	.tile {
		background: var(--c-tile);
		border: 1px solid color-mix(in srgb, var(--color-foreground) 7%, transparent);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--tile-shadow);
	}
	.day-bar button {
		border: 0;
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 14px;
		border-radius: 999px;
		cursor: pointer;
	}
	.day-bar button:hover:not(:disabled) {
		background: var(--color-surface);
	}
	.day-bar button:disabled {
		color: var(--color-foreground-subtle);
		cursor: default;
	}
	.step {
		width: 30px;
		height: 30px;
		font-size: 18px !important;
		line-height: 1;
	}
	.date {
		padding: 6px 12px;
		min-width: 120px;
	}
	.today {
		padding: 6px 12px;
		color: var(--color-primary) !important;
	}
	.picker {
		position: absolute;
		left: 50%;
		bottom: 0;
		width: 0;
		height: 0;
		opacity: 0;
		pointer-events: none;
	}
	/* Under the day bar, not over the middle: the map holds the last known
	   position at its centre, and the note must not hide it. */
	.note {
		position: absolute;
		top: 72px;
		left: 50%;
		transform: translateX(-50%);
		padding: 8px 14px;
		border-radius: 12px;
		text-align: center;
	}
	.note p {
		margin: 0;
	}
	.note-title {
		color: var(--color-foreground);
		font-size: 14px;
	}
	.note-line {
		color: var(--color-foreground-muted);
		font-size: 12px;
	}
	.rail-error {
		position: absolute;
		top: 16px;
		right: 16px;
		max-width: min(384px, 42%);
		margin: 0;
		padding: 12px 16px;
		border-radius: 12px;
		font-size: 13px;
		color: var(--color-foreground);
	}
	.no-basemap {
		position: absolute;
		left: 12px;
		bottom: 10px;
		margin: 0;
		color: var(--color-foreground-subtle);
		font-size: 12px;
	}
</style>
