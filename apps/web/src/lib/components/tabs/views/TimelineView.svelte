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
	import type { GeoJSONSource, Map as MlMap, Marker, StyleSpecification } from 'maplibre-gl';
	import { fetchDayBounds, fetchDayWindow, fetchDerived, lastMeasured, localDay, quietDayVerdict, stepDay } from '$lib/timeline/day';
	import { cleanTrack, dropSpikes, flagHoles, splitTrack, toFixes, type Fix, type Line } from '$lib/timeline/track';
	import { buildRail, type RailPick, type RailSection } from '$lib/timeline/rail';
	import { anchorAt, positionAt, trackMetres } from '$lib/timeline/anchor';
	import { Bubbles, type Area, type MapMoment } from '$lib/timeline/bubbles';
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
	let railError = $state(false);
	/** The day's cleaned track: where a picked moment is found on the map. */
	let dayTrack: Fix[] = [];
	/** The day's fixes, for framing the whole day. */
	let dayFixes: Fix[] = [];
	/** The playhead: a day opens at its midnight, untouched, until the first
	 *  pick (main.js:1162, 1387). Everything follows it: the pin, the lit
	 *  bubble, the lit rail section and row, the lit stretch of path. */
	let playT = $state(0);
	let dayStart = 0;
	let dayEnd = 0;
	let pin: Marker | null = null;
	let bubbles: Bubbles | null = null;
	/** Which stretch of path is lit, so it is written only when it changes. */
	let litKey = '';
	/** What the user last pointed at, for the rail's scroll (main.js:1559). */
	let railFocus = $state<'row' | 'sec'>('row');

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
				// The lit stretch (main.js:976-981): the playhead's moving moment in
				// the talk colour, else its drive in the move colour, at the path's
				// own weight - the colour marks it, the width says nothing.
				m.addSource('lit-run', { type: 'geojson', data: lines([]) });
				m.addSource('lit-bridge', { type: 'geojson', data: lines([]) });
				m.addLayer({
					id: 'lit-run',
					type: 'line',
					source: 'lit-run',
					layout: { 'line-join': 'round', 'line-cap': 'round' },
					paint: { 'line-color': ['coalesce', ['get', 'color'], COLOURS.talk], 'line-width': 3, 'line-opacity': 0.95 },
				});
				m.addLayer({
					id: 'lit-bridge',
					type: 'line',
					source: 'lit-bridge',
					layout: { 'line-join': 'round', 'line-cap': 'butt' },
					paint: {
						'line-color': ['coalesce', ['get', 'color'], COLOURS.talk],
						'line-width': 1.5,
						'line-opacity': 0.9,
						'line-dasharray': [2, 2.5],
					},
				});
				const el = document.createElement('div');
				el.className = 'tl-pin';
				pin = new ml.Marker({ element: el }).setLngLat([0, 0]);
				bubbles = new Bubbles(ml, m, {
					area: () => clearArea(m),
					pad: () => fitPad(m),
					time: clock,
					place: placeAt,
					onpick: (s) => {
						railFocus = 'row';
						park(s);
					},
					fitDay: () => frameDay(m, 650),
					showPoint: (ll) => showPoint(m, [ll.lng, ll.lat]),
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
		dayFixes = fixes;
		dayTrack = cleanTrack(fixes);
		const { runs, bridges } = splitTrack(dayTrack);
		(m.getSource('track-run') as GeoJSONSource).setData(lines(runs));
		(m.getSource('track-bridge') as GeoJSONSource).setData(lines(bridges));
		dayStart = startMs;
		dayEnd = endMs;
		playT = startMs;
		railFocus = 'row';
		litKey = '';
		// The rail draws first, so the framing can leave room for it.
		await tick();
		if (mine !== asked) return;
		if (fixes.length) {
			status = 'shown';
			frameDay(m, 0);
			bubbles?.build(mapMoments());
			syncPlayhead();
			return;
		}
		bubbles?.clear();
		syncPlayhead();
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

	/** Frame the whole day's fixes (`v4Fit`, main.js:1185-1189). */
	function frameDay(m: MlMap, duration: number) {
		const ml = maplibre;
		if (!ml || !dayFixes.length) return;
		const b = new ml.LngLatBounds();
		for (const f of dayFixes) b.extend([f.lng, f.lat]);
		m.fitBounds(b, { padding: fitPad(m), maxZoom: 15, duration });
	}

	/** The day's conversations and walks at their anchors, each moving when it
	 *  covered over half a kilometre of track (main.js:1028-1039). */
	function mapMoments(): MapMoment[] {
		const out: MapMoment[] = [];
		for (const r of sections.flatMap((s) => s.rows)) {
			const at = anchorAt(dayTrack, r.s, r.e);
			if (!at) continue;
			const km = trackMetres(dayTrack, r.s, r.e) / 1000;
			out.push({ s: r.s, e: r.e, kind: r.kind, title: r.title, lng: at.lng, lat: at.lat, km, moving: km > 0.5 });
		}
		return out;
	}

	/** "12:20 PM" on the day's clock. */
	const clock = (ms: number) => new Date(ms).toLocaleTimeString('en-US', { timeZone: zone, hour: 'numeric', minute: '2-digit' });

	/** The stay holding `ms`, by name; null in a drive or a gap (main.js:1125). */
	function placeAt(ms: number): string | null {
		const where = sections.find((s) => s.kind !== 'sleep' && s.s <= ms && ms < s.e);
		return where?.kind === 'place' ? where.title : null;
	}

	/** Move the playhead: a pick lands exactly there, inside the day, a hair
	 *  before midnight at most (`v4Seek`, main.js:491). */
	function park(t: number) {
		playT = Math.max(dayStart, Math.min(dayEnd - 1000, t));
		syncPlayhead();
	}

	/** Everything that follows the playhead on the map (main.js:1004-1016,
	 *  1103-1113): the pin, the lit bubble, the lit stretch of path. */
	function syncPlayhead() {
		const m = map;
		if (!m) return;
		const at = positionAt(dayTrack, playT);
		if (at && pin) pin.setLngLat([at.lng, at.lat]).addTo(m);
		else pin?.remove();
		bubbles?.sync(playT);
		const moving = bubbles?.movingCurrent() ?? null;
		const drive = moving ? null : sections.find((s) => s.kind === 'transit' && s.s <= playT && playT < s.e);
		const seg = moving ?? drive ?? null;
		const key = moving ? `m${moving.s}` : drive ? `t${drive.s}` : '';
		if (key === litKey) return;
		litKey = key;
		const pts = seg ? dayTrack.filter((f) => f.t >= seg.s && f.t < seg.e) : [];
		const { runs, bridges } = splitTrack(pts);
		const color = moving ? COLOURS.talk : COLOURS.move;
		const run = pts.length >= 2 ? lines(runs) : lines([]);
		const bridge = pts.length >= 2 ? lines(bridges) : lines([]);
		run.properties = { color };
		bridge.properties = { color };
		(m.getSource('lit-run') as GeoJSONSource | undefined)?.setData(run);
		(m.getSource('lit-bridge') as GeoJSONSource | undefined)?.setData(bridge);
	}

	/** A rail pick parks the playhead and takes the map there (main.js:1634-1637,
	 *  1087-1101): a row reveals its bubble; a section parks at its start (not
	 *  when it is already the live one) and a drive is framed whole, anything
	 *  else opens its bubbles' chip or pans to where the track says you were. */
	function reveal(p: RailPick) {
		const m = map;
		const ml = maplibre;
		if (!m || !ml) return;
		if (p.kind === 'conversation' || p.kind === 'walk') {
			railFocus = 'row';
			park(p.s);
			bubbles?.reveal(p.s);
			return;
		}
		const i = sections.findIndex((s) => s.kind === p.kind && s.s === p.s);
		const live = sections.findLastIndex((s) => s.s <= playT && playT < s.e);
		railFocus = 'sec';
		if (i !== live) park(p.s);
		if (p.kind === 'transit') {
			bubbles?.fold();
			const path = dayTrack.filter((f) => f.t >= p.s && f.t < p.e);
			if (path.length >= 2) {
				const b = new ml.LngLatBounds();
				for (const f of path) b.extend([f.lng, f.lat]);
				m.fitBounds(b, { padding: fitPad(m), maxZoom: 15, duration: 650 });
				return;
			}
		}
		const keys = sections[i]?.rows.map((r) => r.s) ?? [];
		if (bubbles?.revealSection(keys)) return;
		const at = anchorAt(dayTrack, p.s, p.e);
		if (at) showPoint(m, [at.lng, at.lat]);
	}

	/** The clear map (`v4ClearArea`, main.js:1042-1044): not under the day bar
	 *  or the rail, 8 px in from every edge. */
	function clearArea(m: MlMap): Area {
		const c = m.getContainer();
		const bar = dayBar?.getBoundingClientRect();
		const top = bar ? bar.bottom - c.getBoundingClientRect().top : 58;
		const railW = rail?.width() ?? 0;
		return { l: 8, t: top + 8, r: c.clientWidth - (railW ? railW + 20 : 0) - 8, b: c.clientHeight - 8 };
	}

	/** Bring one spot onto the clear map (`v4ShowPoint`, main.js:1092-1094):
	 *  nothing if it is already there, else ease it to the centre of the clear
	 *  area at the same zoom - a pan, never a re-frame. Whether it moved. */
	function showPoint(m: MlMap, ll: [number, number]): boolean {
		const lim = clearArea(m);
		const c = m.getContainer();
		const p = m.project(ll);
		if (p.x > lim.l + 40 && p.x < lim.r - 40 && p.y > lim.t + 40 && p.y < lim.b - 40) return false;
		const offset: [number, number] = [(lim.l + lim.r) / 2 - c.clientWidth / 2, (lim.t + lim.b) / 2 - c.clientHeight / 2];
		m.easeTo({ center: ll, offset, duration: 650 });
		return true;
	}

	onDestroy(() => {
		bubbles?.destroy();
		bubbles = null;
		pin?.remove();
		pin = null;
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

	<TimelineRail bind:this={rail} {sections} {zone} {playT} focus={railFocus} onpick={reveal} />
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
	/* The map's own marks, made by MapLibre markers outside this component's
	   markup, so :global. The prototype's pin, bubbles, chips and place card
	   (dayback/index.html:104-129). */
	.timeline :global(.tl-pin) {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: var(--c-place);
		border: 3px solid var(--c-ring);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--pin-shadow);
	}
	.timeline :global(.tl-bubble) {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: var(--c-tile);
		border: 2px solid var(--c-mark);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--mark-shadow);
		cursor: pointer;
		z-index: 1;
	}
	.timeline :global(.tl-bubble.cur) {
		width: 12px;
		height: 12px;
		background: var(--c-place);
		border-color: var(--c-ring);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--mark-cur-shadow);
		z-index: 6;
	}
	/* Every moment's label surfaces; the layout puts it in a free slot around
	   its dot, and the label is part of the click target. */
	.timeline :global(.tl-bubble-label) {
		display: flex;
		flex-direction: column;
		/* design-ok: the Dayback prototype's 1 px line gap (owner's call, 2026-09-30) */
		gap: 1px;
		position: absolute;
		left: 50%;
		top: 50%;
		transform: translate(-50%, calc(-50% - 30px));
		width: max-content;
		max-width: 200px;
		text-align: center;
		background: var(--c-tile);
		border-radius: 12px;
		/* design-ok: the Dayback prototype's label padding (owner's call, 2026-09-30) */
		padding: 5px 11px;
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--label-shadow);
		font-family: var(--font-sans);
		font-size: 11.5px;
		font-weight: 600;
		letter-spacing: -0.01em;
		line-height: 1.22;
		color: var(--color-foreground);
		cursor: pointer;
		z-index: 2;
	}
	.timeline :global(.tl-bubble-label i) {
		font-style: normal;
		font-weight: 500;
		font-size: 10px;
		color: var(--color-foreground-muted);
	}
	.timeline :global(.tl-bubble.cur .tl-bubble-label) {
		font-size: 12.5px;
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--label-cur-shadow);
	}
	.timeline :global(.tl-bubble-line) {
		position: absolute;
		left: 50%;
		top: 50%;
		width: 400px;
		height: 400px;
		transform: translate(-50%, -50%);
		overflow: visible;
		pointer-events: none;
		z-index: 0;
	}
	.timeline :global(.tl-bubble-line line) {
		stroke: var(--c-leader);
		stroke-width: 1.4;
		stroke-linecap: round;
	}
	.timeline :global(.tl-bubble.cur .tl-bubble-line line) {
		stroke: var(--c-place);
		stroke-width: 1.7;
	}
	/* A card member keeps its shared dot and drops its label and line. */
	.timeline :global(.tl-bubble.in-card .tl-bubble-label),
	.timeline :global(.tl-bubble.in-card .tl-bubble-line) {
		display: none;
	}
	/* A knot of moments folded into a count chip; never set transform here,
	   MapLibre positions the marker with it. */
	.timeline :global(.tl-knot) {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		border: 2px solid var(--c-ring);
		padding: 0;
		background: var(--c-knot);
		color: var(--c-ring);
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 600;
		line-height: 24px;
		text-align: center;
		font-variant-numeric: tabular-nums;
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--knot-shadow);
		cursor: pointer;
		z-index: 5;
	}
	.timeline :global(.tl-knot:hover) {
		filter: brightness(1.12);
	}
	.timeline :global(.tl-knot.cur) {
		background: var(--c-place);
	}
	/* One callout for a shared spot, its moments as rows. */
	.timeline :global(.tl-card) {
		width: max-content;
		min-width: 230px;
		max-width: 310px;
		background: var(--c-tile);
		border-radius: 12px;
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--card-shadow);
		font-family: var(--font-sans);
		color: var(--color-foreground);
		z-index: 7;
	}
	.timeline :global(.tl-card-hd) {
		display: flex;
		align-items: baseline;
		gap: 8px;
		/* design-ok: the Dayback prototype's card padding (owner's call, 2026-09-30) */
		padding: 10px 10px 8px 14px;
		border-bottom: 1px solid color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	.timeline :global(.tl-card-hd b) {
		font-size: 13px;
		font-weight: 700;
		letter-spacing: -0.01em;
		white-space: nowrap;
	}
	.timeline :global(.tl-card-hd span) {
		font-size: 11px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.timeline :global(.tl-card-x) {
		margin-left: auto;
		border: 0;
		background: none;
		color: var(--color-foreground-subtle);
		font-size: 16px;
		line-height: 1;
		cursor: pointer;
		/* design-ok: the Dayback prototype's 2 px mark (owner's call, 2026-09-30) */
		padding: 0 2px;
	}
	.timeline :global(.tl-card-x:hover) {
		color: var(--color-foreground);
	}
	.timeline :global(.tl-card-rows) {
		overflow: auto;
		/* design-ok: the Dayback prototype's list inset (owner's call, 2026-09-30) */
		padding: 4px 0;
	}
	.timeline :global(.tl-card-row) {
		display: flex;
		align-items: baseline;
		/* design-ok: the Dayback prototype's row gap (owner's call, 2026-09-30) */
		gap: 10px;
		width: 100%;
		text-align: left;
		border: 0;
		background: none;
		/* design-ok: the Dayback prototype's row padding (owner's call, 2026-09-30) */
		padding: 6px 14px;
		cursor: pointer;
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}
	.timeline :global(.tl-card-row:hover) {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.timeline :global(.tl-card-row i) {
		font-style: normal;
		font-size: 11px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		flex: 0 0 58px;
	}
	.timeline :global(.tl-card-row span) {
		font-size: 12.5px;
		font-weight: 600;
		letter-spacing: -0.01em;
		line-height: 1.25;
	}
	.timeline :global(.tl-card-row.cur i),
	.timeline :global(.tl-card-row.cur span) {
		color: var(--c-place);
	}
	/* The pointer from the card to its dot. */
	.timeline :global(.tl-card-tail) {
		position: absolute;
		left: 50%;
		/* design-ok: the Dayback prototype's card tail (owner's call, 2026-09-30) */
		bottom: -6px;
		width: 12px;
		height: 12px;
		background: var(--c-tile);
		transform: translateX(-50%) rotate(45deg);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--card-tail-shadow);
	}
</style>
