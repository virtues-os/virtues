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
	import { onMount, onDestroy } from 'svelte';
	import { browser } from '$app/environment';
	import { atlasStyle } from '$lib/map/atlas';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import type { GeoJSONSource, Map as MlMap, StyleSpecification } from 'maplibre-gl';
	import { fetchDayWindow, lastMeasured, localDay, quietDayVerdict, stepDay } from '$lib/timeline/day';
	import { cleanTrack, dropSpikes, flagHoles, splitTrack, toFixes, type Line } from '$lib/timeline/track';
	import { APPLE_LIGHT, hsl, recolour } from '$lib/timeline/palette';

	// The prototype's place colour, `--c-place` (dayback/index.html:17): path and pin
	// are location, so they wear it (dayback/src/main.js:1755).
	const TRACK = '#4C86D6';
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
		const { startMs, endMs } = localDay(slug);
		let window;
		try {
			window = await fetchDayWindow(startMs, endMs);
		} catch (e) {
			console.warn('[Timeline] day fetch failed', e);
			if (mine === asked) status = 'error';
			return;
		}
		if (mine !== asked) return;
		// Spikes go first, then holes are judged across the whole window, then the
		// day is cut out of it.
		const all = flagHoles(dropSpikes(toFixes(window.points)));
		const fixes = all.filter((f) => f.t >= startMs && f.t < endMs);
		const { runs, bridges } = splitTrack(cleanTrack(fixes));
		(m.getSource('track-run') as GeoJSONSource).setData(lines(runs));
		(m.getSource('track-bridge') as GeoJSONSource).setData(lines(bridges));
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
		const measured = last ? lastMeasured(last.t, false) : null;
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
	 *  the measured top bar plus a buffer of max(34 px, 6 % of the smaller side)
	 *  on every edge, each capped so fitBounds can still move. The scrubber and
	 *  the rail, once built, reserve their space here too. */
	function fitPad(m: MlMap) {
		const c = m.getContainer();
		const cw = c.clientWidth || 900;
		const ch = c.clientHeight || 600;
		const buf = Math.max(34, Math.round(Math.min(cw, ch) * 0.06));
		const bar = dayBar?.getBoundingClientRect();
		const top = bar ? bar.bottom - c.getBoundingClientRect().top : 58;
		return {
			top: Math.min(top + buf, ch * 0.34),
			bottom: Math.min(buf, ch * 0.45),
			left: Math.min(buf, cw * 0.4),
			right: Math.min(buf, cw * 0.5),
		};
	}

	onDestroy(() => {
		resizer?.disconnect();
		resizer = null;
		map?.remove();
		map = null;
	});
</script>

<div class="timeline">
	<div class="timeline-map" bind:this={container}></div>

	<div class="day-bar" bind:this={dayBar}>
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

	{#if status === 'empty' && note}
		<div class="note">
			<p class="note-title">{note.title}</p>
			{#each note.lines as line (line)}
				<p class="note-line">{line}</p>
			{/each}
		</div>
	{:else if status === 'error'}
		<div class="note"><p class="note-title">Your server couldn't load {label}. Reload the page to try again.</p></div>
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
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
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
		/* Floats over the map, so it takes the elevated surface; it holds no
		   control, so no border (agents/build/design-grammar.md). */
		background: var(--color-surface-elevated);
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
	.no-basemap {
		position: absolute;
		left: 12px;
		bottom: 10px;
		margin: 0;
		color: var(--color-foreground-subtle);
		font-size: 12px;
	}
</style>
