<!--
	TimelineView.svelte — the Timeline, map mode.

	A MapLibre map on the box's own atlas tiles (Protomaps, served from
	/api/map/vt) showing one local day: its GPS path, solid runs of recorded
	fixes and dashed bridges across the holes in the recording
	($lib/timeline/track); the stays, drives and moments the server derives
	(/api/timeline/derived) as the inspector beside it and bubbles on it; the day's
	streams in the scrubber under it. The date card steps between days.

	Its own map (MapLibre v6), not the Leaflet MovementMap: the prototype's map
	logic is MapLibre-native, and this is where it lives.
-->
<script lang="ts">
	import { onMount, onDestroy, tick, untrack } from 'svelte';
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { browser } from '$app/environment';
	import { atlasSquares, atlasStyle } from '$lib/map/atlas';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import type { GeoJSONSource, LngLat, Map as MlMap, Marker, StyleSpecification } from 'maplibre-gl';
	import { fetchDay, localDay, quietDay, stepDay } from '$lib/timeline/day';
	import { deriveDay, joinDays, type Sources } from '$lib/timeline/derive';
	import { getStreamHealth } from '$lib/api/client';
	import { mergePlace, updatePlace, type NearbyPlace } from '$lib/wiki/api';
	import type { DerivedPlace, DerivedWindow, VoiceWindow } from '$lib/timeline/inspector';
	import type { LaneWindow } from '$lib/timeline/lanes';
	import { cleanTrack, dropSpikes, flagHoles, splitTrack, toFixes, type Fix, type Line } from '$lib/timeline/track';
	import { wheelSource } from '$lib/timeline/wheel';
	import { buildInspector, placeTitle, type InspectorPick, type InspectorSection } from '$lib/timeline/inspector';
	import { anchorAt, positionAt, trackMetres } from '$lib/timeline/anchor';
	import { Bubbles, type Area, type MapMoment } from '$lib/timeline/bubbles';
	import TimelineInspector from '$lib/components/timeline/TimelineInspector.svelte';
	import ReviewTile from '$lib/components/timeline/ReviewTile.svelte';
	import TimelineScrubber from '$lib/components/timeline/TimelineScrubber.svelte';
	import TimelineMonth from '$lib/components/timeline/TimelineMonth.svelte';
	import { ReviewState } from '$lib/timeline/reviewState.svelte';
	import { sendAnswer, type PlaceCandidate } from '$lib/timeline/review';
	import Icon from '$lib/components/Icon.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Pill from '$lib/components/timeline/Pill.svelte';
	import { laneData, NO_LANES, type Lanes, type RibbonKind } from '$lib/timeline/lanes';
	import { buildFolds, clampView, HOUR, MIN, midnightIn, tierOf, tierView, unwarp, warp, weekOf, mondayOf, zoneOffset, type Tier } from '$lib/timeline/scale';
	import { colourVars, mapInk } from '$lib/timeline/colours';
	import { APPLE_DARK, APPLE_LIGHT, hsl, recolour, type Palette } from '$lib/timeline/palette';

	/** Below this pane width the list leaves the map's side for a sheet. */
	const NARROW_PX = 640;
	/** The narrow layout's sheet, as a share of the pane's height. */
	const SHEET = 0.34;

	// Without the box's map archives there is no basemap: the palette's
	// ground alone, so the path still draws and the page doesn't read as broken.
	const bare = (palette: Palette): StyleSpecification => ({
		version: 8,
		sources: {},
		layers: [{ id: 'bare', type: 'background', paint: { 'background-color': hsl(palette.land) } }],
	});

	/** Virtues' own scheme: its dark themes set `--identity-dark: 1` (themes.css),
	 *  and a theme switch fires `themechange` (utils/theme.ts), as the Home day
	 *  page reads it. The map follows with the atlas's own dark style. */
	const isDark = () => getComputedStyle(document.documentElement).getPropertyValue('--identity-dark').trim() === '1';
	let dark = $state(browser ? isDark() : false);
	/** The basemap for a side: the atlas's own style in the prototype's
	 *  palette (cream land, white streets, green parks, blue water; see
	 *  palette.ts), or the palette's plain ground when the box has no map
	 *  files. When
	 *  the box can't be reached (it's restarting), the ground stands in, no note
	 *  claims there are no maps, and the map asks again until the box answers. */
	async function baseStyle(night: boolean): Promise<StyleSpecification> {
		const palette = night ? APPLE_DARK : APPLE_LIGHT;
		let style: StyleSpecification | null;
		try {
			style = await atlasStyle(night ? 'dark' : 'light');
		} catch (e) {
			console.warn('[Timeline] map files unreachable; asking again', e);
			clearTimeout(mapRetry);
			mapRetry = window.setTimeout(() => void restyle(), MAP_RETRY_MS);
			return bare(palette);
		}
		basemap = style !== null;
		if (squares === null) void loadSquares();
		return style ? recolour(style, palette) : bare(palette);
	}
	/** How long to wait before asking an unreachable box for its map again. */
	const MAP_RETRY_MS = 5000;
	let mapRetry = 0;
	/** The basemap the map is drawn on: every source and layer not in it is
	 *  the day's own. */
	let base: StyleSpecification | null = null;
	/** A theme switch swaps the basemap and carries the day's own sources and
	 *  layers across, so nothing reloads. */
	async function restyle() {
		const m = map;
		if (!m) return;
		const night = dark;
		const next = await baseStyle(night);
		if (map !== m || dark !== night) return;
		const was = base;
		base = next;
		m.setStyle(next, {
			transformStyle: (prev, fresh) => {
				if (!prev || !was) return fresh;
				const sources = { ...fresh.sources };
				for (const [id, src] of Object.entries(prev.sources)) if (!(id in was.sources)) sources[id] = src;
				const baseLayers = new Set(was.layers.map((l) => l.id));
				return { ...fresh, sources, layers: [...fresh.layers, ...prev.layers.filter((l) => !baseLayers.has(l.id))] };
			},
		});
	}
	function onThemeChange() {
		const d = isDark();
		if (d === dark) return;
		dark = d;
		void restyle();
		paintInk();
	}
	$effect(() => {
		window.addEventListener('themechange', onThemeChange);
		return () => window.removeEventListener('themechange', onThemeChange);
	});

	let container: HTMLDivElement;
	let root = $state<HTMLElement | null>(null);
	/** The pane's own width (split view, a small window), which decides where
	 *  the top cards go. */
	let paneW = $state(0);
	/** The date card and its month float on the map; its measures place
	 *  everything else. */
	let dateWrap = $state<HTMLElement | null>(null);
	let cardW = $state(0);
	let cardH = $state(0);
	let scrubH = $state(0);
	let monthOpen = $state(false);
	/** The bottom of the date card, for the map's framing and clear area. */
	const topChrome = $derived(16 + cardH);
	/** A pane too narrow for the list beside the map (a phone, a split view)
	 *  puts it in a sheet across the bottom, under a full-width scrubber. */
	const narrow = $derived(paneW > 0 && paneW < NARROW_PX);
	/** The inspector runs full height, unless the date card would run into it. */
	const inspectorLow = $derived(paneW > 0 && 16 + cardW + 16 > paneW - Math.min(384, paneW * 0.42) - 16);
	let note = $state<{ title: string; lines: string[] } | null>(null);
	let map: MlMap | null = null;
	/** The map's paint, read off the theme (colours.ts). */
	let ink = browser ? mapInk() : { track: '', move: '', movePicked: '', label: '', halo: '' };
	/** A theme switch repaints the day's own layers. */
	function paintInk() {
		ink = mapInk();
		const m = map;
		if (!m?.getLayer('track-run')) return;
		for (const id of ['track-run', 'track-bridge']) m.setPaintProperty(id, 'line-color', ink.track);
		m.setPaintProperty('track-drive', 'line-color', ink.move);
		m.setPaintProperty('review-dots', 'circle-color', ink.label);
		m.setPaintProperty('review-dots', 'circle-stroke-color', ink.halo);
		m.setPaintProperty('review-names', 'text-color', ink.label);
		m.setPaintProperty('review-names', 'text-halo-color', ink.halo);
		m.setPaintProperty('lit-glow', 'line-color', ink.movePicked);
		for (const id of ['lit-run', 'lit-bridge']) m.setPaintProperty(id, 'line-color', picked?.kind === 'transit' ? ink.movePicked : ink.move);
	}
	let maplibre: typeof import('maplibre-gl') | null = null;
	let resizer: ResizeObserver | null = null;
	let inspector = $state<ReturnType<typeof TimelineInspector> | null>(null);
	let sections = $state<InspectorSection[]>([]);
	/** Each place's centre, [lng, lat], by its id: where a picked stay flies in to. */
	let placeCoords = new Map<string, [number, number]>();
	let zone = $state('UTC');
	/** The day's cleaned track: where a picked moment is found on the map. */
	let dayTrack: Fix[] = [];
	/** The day's fixes, for framing the whole day. */
	let dayFixes: Fix[] = [];
	/** The playhead: a day opens at its midnight, untouched, until the first
	 *  pick (main.js:1162, 1387). Everything follows it: the pin, the lit
	 *  bubble, the lit inspector section and row, the lit stretch of path. */
	let playT = $state(0);
	/** The user has moved the playhead in this day; until then the inspector opens
	 *  nothing by itself (main.js:1559). */
	let armed = $state(false);
	let dayStart = $state(0);
	let dayEnd = $state(0);
	/** The calendar week holding the day (Monday to Sunday): the Week span's
	 *  bounds, inside which the playhead may move to another day. */
	let weekStart = $state(0);
	/** The Monday of the loaded day's week, YYYY-MM-DD. */
	let weekSlug = '';
	let weekEnd = $state(0);
	/** The playhead crossed into another day of the week: that day loads
	 *  around it, keeping the playhead and the week in view. */
	let crossing = false;
	let pin: Marker | null = null;
	let bubbles: Bubbles | null = null;
	/** Which stretch of path is lit, so it is written only when it changes. */
	let litKey = '';
	/** What the user last pointed at, for the inspector's scroll (main.js:1559). */
	let inspectorFocus = $state<'row' | 'sec'>('row');

	/** The scrubber's view, the span on screen (main.js:10). It stays put while
	 *  the playhead moves, and scrolls only to keep the playhead in it. */
	let viewStart = $state(0);
	let viewEnd = $state(0);
	let playing = $state(false);
	/** What the lanes draw: the day, then its week once the view leaves the
	 *  day. */
	let lanes = $state<Lanes>(NO_LANES);
	/** The local midnights around the day: a date no night touches folds
	 *  01:00-06:00 instead (main.js:103-104). */
	let midnights = $state<number[]>([]);
	const folds = $derived(buildFolds(lanes.nights, midnights));
	/** The tier is what the span reads as, however it got there (main.js:81). */
	const tier = $derived(tierOf(viewEnd - viewStart));
	/** A day or week nudge crosses to another day, keeping the time of day and
	 *  the tier; it lands once that day has loaded (main.js:1385-1386). */
	let pendingNudge: { tier: Tier; tod: number } | null = null;

	let { tab }: { tab: Tab; active: boolean } = $props();

	const today = getLocalDateSlug();
	let date = $state(today);
	// `/timeline/YYYY-MM-DD` (the day page's "Open this day in the Timeline")
	// opens that day, then the route goes back to `/timeline`: one Timeline
	// tab, which the rail's tile finds again. The write only ever changes the
	// value, so this effect cannot feed itself.
	$effect(() => {
		const asked = tab.route.match(/^\/timeline\/(\d{4}-\d{2}-\d{2})$/)?.[1];
		if (!asked) return;
		untrack(() => {
			date = asked;
			windowShellStore.updateTab(tab.id, { route: '/timeline' });
		});
	});
	let ready = $state(false);
	let basemap = $state(true);
	/** The squares the box holds street maps for; null until known, or when the
	 *  box can't say. Outside them the map has only the world overview. */
	let squares: number[][] | null = null;
	/** The map's centre, zoomed past the world overview, sits outside every
	 *  street-map square: the bare map says why. */
	let streetless = $state(false);
	/** Ask the box which squares hold street maps (again, after it couldn't be reached). */
	function loadSquares() {
		return atlasSquares().then((sq) => {
			squares = sq;
			syncStreets();
		});
	}
	function syncStreets() {
		const m = map;
		if (!m || !squares) {
			streetless = false;
			return;
		}
		const c = m.getCenter();
		streetless = m.getZoom() > 7 && !squares.some(([w, s, e, n]) => c.lng >= w && c.lng <= e && c.lat >= s && c.lat <= n);
	}
	let status = $state<'loading' | 'shown' | 'empty' | 'error'>('loading');
	/** On a day with no location whose list would only repeat the note's
	 *  "No location recorded", the note on the map says it once and the list
	 *  stays away. A gap holding conversations keeps the list: they are the
	 *  day's record. */
	const shownSections = $derived(
		status === 'empty' && note && sections.every((x) => x.kind === 'gap' && !x.rows.length) ? [] : sections,
	);
	let asked = 0; // the latest day asked for, so a slow answer can't overwrite a newer one

	/** The date card's date (dayback/src/main.js:1531-1534): "Tuesday, July
	 *  28", with the year when it isn't this one. */
	const title = $derived.by(() => {
		const d = new Date(localDay(date).startMs);
		const thisYear = d.getFullYear() === new Date().getFullYear();
		return d.toLocaleDateString('en-US', {
			weekday: 'long',
			month: 'long',
			day: 'numeric',
			...(thisYear ? {} : { year: 'numeric' }),
		});
	});

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
			const style = await baseStyle(dark);
			if (cancelled || !container) return;
			base = style;
			const m = new ml.Map({
				container,
				style,
				center: [0, 20],
				zoom: 1.5,
				// v6: attributionControl takes options (or false), not a bare boolean.
				attributionControl: {},
			});
			map = m;
			// The prototype's gestures (dayback/src/main.js:988-993, 1870-1875): a
			// pinch zooms, two fingers pan, nothing rotates, and no
			// double-click, drag-box or keyboard zoom (the keys belong to the
			// playhead). A mouse's wheel zooms (onWheel).
			m.doubleClickZoom.disable();
			m.boxZoom.disable();
			m.keyboard.disable();
			m.dragRotate.disable();
			m.touchZoomRotate.disableRotation();
			// A pinch follows the fingers: spreading them twice as far zooms
			// the map in twice (one zoom level). Browsers send a pinch as a
			// ctrl wheel with deltaY = -100 * ln(scale) (Chrome's
			// touchpad_pinch_event_queue.cc; Firefox alike), and MapLibre's
			// scale per frame is about exp(|delta| * rate / 2), so 1/50 is
			// 1:1; its default, 1/100, moved the map half as far (the owner's
			// call, 2026-10-09: match Apple Maps). A mouse's wheel zooms at
			// 1/360, a quarter faster than MapLibre's 1/450.
			m.scrollZoom.setZoomRate(PINCH_RATE.trackpad);
			m.scrollZoom.setWheelZoomRate(PINCH_RATE.wheel);
			container.addEventListener('wheel', onWheel, { passive: false, capture: true });
			m.on('move', syncAway);
			// A drag or a pinch by hand stops following until the next scrub or play.
			m.on('movestart', (e) => {
				if ((e as { originalEvent?: Event }).originalEvent) following = false;
			});
			m.on('moveend', syncStreets);
			// A click on the day's path picks the stay or drive it belongs to, as
			// its row in the list does (the owner's call, 2026-10-08; the
			// prototype's path took no clicks). A click on the map anywhere else
			// (not a bubble, a chip or a card) lets go of a picked drive or stay.
			m.on('click', (e) => {
				// Pointing to Home or Work: the tap is the answer, and nothing else.
				if (pointing) return tapAt(e.lngLat.lat, e.lngLat.lng);
				const sec = m.queryRenderedFeatures(e.point, { layers: PATH_HIT }).length ? sectionAt(m, e.point) : null;
				if (sec) return reveal({ kind: sec.kind, s: sec.s, e: sec.e });
				if (picked) unpick();
			});
			for (const id of PATH_HIT) {
				m.on('mouseenter', id, () => !pointing && (m.getCanvas().style.cursor = 'pointer'));
				m.on('mouseleave', id, () => {
					if (!pointing) m.getCanvas().style.cursor = '';
					hoverPath(m, null);
				});
				m.on('mousemove', id, (e) => hoverPath(m, e.point));
			}
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
					paint: { 'line-color': ink.track, 'line-width': 3, 'line-opacity': 0.55 },
				});
				// A hole in the recording: thin and dashed, so the straight hop
				// reads as a bridge, not a route.
				m.addLayer({
					id: 'track-bridge',
					type: 'line',
					source: 'track-bridge',
					layout: { 'line-join': 'round', 'line-cap': 'butt' },
					paint: { 'line-color': ink.track, 'line-width': 1.5, 'line-opacity': 0.4, 'line-dasharray': [2, 3] },
				});
				// Every drive in moving's hue over the path, so the map tells being
				// somewhere from going somewhere as the lanes and the list do.
				m.addSource('track-drive', { type: 'geojson', data: lines([]) });
				m.addLayer({
					id: 'track-drive',
					type: 'line',
					source: 'track-drive',
					layout: { 'line-join': 'round', 'line-cap': 'round' },
					paint: { 'line-color': ink.move, 'line-width': 3, 'line-opacity': 0.85 },
				});
				// The stretch a click would pick, under the pointer: that stay's or
				// drive's piece of the path at twice the weight, in its own colour.
				// The colour changes at once: MapLibre's default 300 ms fade would
				// draw a stay in the last drive's orange as it comes in.
				m.addSource('hover-run', { type: 'geojson', data: lines([]) });
				m.addLayer({
					id: 'hover-run',
					type: 'line',
					source: 'hover-run',
					layout: { 'line-join': 'round', 'line-cap': 'round' },
					paint: { 'line-color': ink.track, 'line-color-transition': { duration: 0, delay: 0 }, 'line-width': 6, 'line-opacity': 1 },
				});
				// What a click on the path finds: the drawn path is 3 px, too thin
				// to aim at, so an unseen 18 px band over it takes the click. A
				// hole's dashed bridge is no one's, so it takes none.
				for (const [id, source] of [['track-hit', 'track-run'], ['drive-hit', 'track-drive']]) {
					m.addLayer({
						id,
						type: 'line',
						source,
						layout: { 'line-join': 'round', 'line-cap': 'round' },
						paint: { 'line-color': ink.track, 'line-width': 18, 'line-opacity': 0 },
					});
				}
				// The lit stretch: the stretch of path the playhead is moving along,
				// in moving's orange at the path's own weight, over a path otherwise
				// ink all day, so only what you're looking at takes the colour. A
				// drive picked from the inspector turns a brighter orange over a
				// soft halo (pick, below).
				m.addSource('lit-run', { type: 'geojson', data: lines([]) });
				m.addSource('lit-bridge', { type: 'geojson', data: lines([]) });
				// A drive picked from the inspector glows under its lit stretch while it
				// stays picked (pick, below); otherwise the glow is off.
				m.addLayer({
					id: 'lit-glow',
					type: 'line',
					source: 'lit-run',
					layout: { 'line-join': 'round', 'line-cap': 'round' },
					paint: { 'line-color': ink.movePicked, 'line-width': 22, 'line-blur': 10, 'line-opacity': 0 },
				});
				m.addLayer({
					id: 'lit-run',
					type: 'line',
					source: 'lit-run',
					layout: { 'line-join': 'round', 'line-cap': 'round' },
					paint: { 'line-color': ink.move, 'line-width': 3, 'line-opacity': 0.95 },
				});
				m.addLayer({
					id: 'lit-bridge',
					type: 'line',
					source: 'lit-bridge',
					layout: { 'line-join': 'round', 'line-cap': 'butt' },
					paint: {
						'line-color': ink.move,
						'line-width': 1.5,
						'line-opacity': 0.9,
						'line-dasharray': [2, 2.5],
					},
				});
				// While "Help Virtues out" asks about a place, the named places it
				// offers are labelled on the map, so each choice can be found
				// (main.js:916-924); otherwise nothing.
				m.addSource('review-names', { type: 'geojson', data: { type: 'FeatureCollection', features: [] } });
				m.addLayer({
					id: 'review-dots',
					type: 'circle',
					source: 'review-names',
					// A named place offered, in the label's ink: a mark on the map,
					// never a second "you were here" beside the pin.
					paint: { 'circle-radius': 3, 'circle-color': ink.label, 'circle-stroke-width': 1.5, 'circle-stroke-color': ink.halo },
				});
				m.addLayer({
					id: 'review-names',
					type: 'symbol',
					source: 'review-names',
					layout: {
						'text-field': ['get', 'name'],
						'text-font': ['Noto Sans Medium'],
						'text-size': 13,
						// Each name finds a side of its dot clear of the others,
						// so a street of bars side by side stays readable.
						'text-variable-anchor': ['top', 'bottom', 'left', 'right', 'top-left', 'top-right', 'bottom-left', 'bottom-right'],
						'text-radial-offset': 0.7,
						'text-justify': 'auto',
						'text-padding': 2,
						'text-max-width': 9,
					},
					paint: { 'text-color': ink.label, 'text-halo-color': ink.halo, 'text-halo-width': 1.5 },
				});
				const el = document.createElement('div');
				el.className = 'tl-pin';
				pin = new ml.Marker({ element: el }).setLngLat([0, 0]);
				bubbles = new Bubbles(ml, m, {
					area: () => clearArea(m),
					pad: () => fitPad(m),
					time: clock,
					place: placeAt,
					onpick: (s) => pickMoment(s, true),
					fitDay: () => frameDay(m, 650),
					flyTo: (ll) => flyIn(m, [ll.lng, ll.lat]),
				});
				ready = true;
			});
		})();
		return () => {
			cancelled = true;
		};
	});

	// The day loads on its own, and the map draws it once the map is up: the
	// bar, the inspector and a day's note never wait on the tiles (the prototype
	// draws its inspector with no map at all).
	$effect(() => {
		void loadDay(date);
	});
	$effect(() => {
		if (!ready) return;
		void drawn; // a new day to draw
		untrack(() => void drawDay());
	});

	/** The last fix before a day with none of its own: where it was last seen. */
	let lastFix: Fix | null = null;
	/** Bumped when a day's data is in, so the map draws it. */
	let drawn = $state(0);

	/** Whether a calendar and a financial account have ever synced: an empty
	 *  lane and one nothing writes to are told apart. */
	let sources: Sources = { calendar: null, finance: null };
	const sourcesReady = getStreamHealth()
		.then((rows) => {
			const ever = (...names: string[]) => rows.some((r) => names.includes(r.name) && r.status !== 'never');
			sources = { calendar: ever('calendar_event'), finance: ever('financial_transaction', 'financial_account') };
		})
		.catch((e) => console.warn('[Timeline] stream health unavailable', e));

	/** The loaded day as derived, so naming a place can redraw it in place. */
	let loaded: { derived: DerivedWindow; voice: VoiceWindow[]; lw: LaneWindow; startMs: number; endMs: number } | null = null;

	/** Redraw the list (and the day's lanes, when they are the day's) after a
	 *  place changed, without reloading or moving the playhead. */
	function redrawPlaces() {
		if (!loaded) return;
		const { derived, voice, lw, startMs, endMs } = loaded;
		sections = buildInspector(derived, startMs, endMs, voice);
		if (lanes.a === startMs && lanes.b === endMs) lanes = laneData(derived, voice, lw, startMs, endMs);
	}

	async function namePlace(placeId: string, name: string): Promise<boolean> {
		if (!(await updatePlace(placeId, { name }))) return false;
		const place = loaded?.derived.places.find((p) => p.id === placeId);
		if (place) Object.assign(place, { place_name: name, is_named: true });
		redrawPlaces();
		return true;
	}

	async function mergeInto(placeId: string, into: NearbyPlace): Promise<boolean> {
		if (!(await mergePlace(placeId, into.id))) return false;
		const derived = loaded?.derived;
		if (derived) {
			const from = derived.places.find((p) => p.id === placeId);
			if (!derived.places.some((p) => p.id === into.id) && from)
				derived.places.push({ ...from, id: into.id, place_name: into.name, is_named: true });
			for (const span of derived.spans) if (span.timeline_place_id === placeId) span.timeline_place_id = into.id;
		}
		redrawPlaces();
		return true;
	}

	async function loadDay(slug: string) {
		const mine = ++asked;
		status = 'loading';
		note = null;
		let day;
		try {
			// The day in the zone it woke up in, whole: its visits, fixes,
			// nights, conversations, steps and calendar.
			[day] = await Promise.all([fetchDay(slug), sourcesReady]);
		} catch (e) {
			console.warn('[Timeline] day fetch failed', e);
			if (mine === asked) status = 'error';
			return;
		}
		const bounds = { startMs: Date.parse(day.started_at), endMs: Date.parse(day.ended_at), zone: day.zone };
		const { derived, voice, lanes: lw } = deriveDay(day, sources);
		loaded = { derived, voice, lw, startMs: Date.parse(day.started_at), endMs: Date.parse(day.ended_at) };
		markAsked(derived.places);
		const window = { points: day.points, before: day.last_point_before };
		if (mine !== asked) return;
		const { startMs, endMs } = bounds;
		zone = bounds.zone;
		sections = buildInspector(derived, startMs, endMs, voice);
		placeCoords = new Map(derived.places.map((p) => [p.id, [p.longitude, p.latitude] as [number, number]]));
		unpick();
		// Spikes go first, then holes are judged across the whole window, then the
		// day is cut out of it.
		const all = flagHoles(dropSpikes(toFixes(window.before ? [window.before, ...window.points] : window.points)));
		const fixes = all.filter((f) => f.t >= startMs && f.t < endMs);
		dayFixes = fixes;
		dayTrack = cleanTrack(fixes);
		dayStart = startMs;
		dayEnd = endMs;
		const week = weekOf(slug, bounds.zone);
		weekSlug = mondayOf(slug);
		weekStart = week.s;
		weekEnd = week.e;
		// Across a crossing the week's lanes already drawn stay, rather than
		// shrinking to the one day until the week reloads.
		const across = crossing;
		// A day opens framed whole; following carries on only across a crossing.
		if (!across) following = false;
		crossing = false;
		if (!(across && lanes.a <= week.s && lanes.b >= week.e)) lanes = laneData(derived, voice, lw, startMs, endMs);
		midnights = Array.from({ length: 15 }, (_, i) => midnightIn(stepDay(slug, i - 7), bounds.zone));
		cancelAnim();
		if (across) {
			// The playhead walked into this day on the Week span: it stays where
			// it is, and so does the week.
			playT = Math.max(startMs, Math.min(endMs - 1000, playT));
			armed = true;
			setView(week.s, week.e);
		} else {
			// A stepped-to day opens whole and untouched, its playhead at wake-up:
			// the end of the night it woke from (recorded or estimated), else
			// the first stay or drive after midnight (the owner's call,
			// 2026-10-08; the prototype opened at midnight, main.js:1387). A
			// nudge lands at its time of day, in its tier.
			playT = wakeOf(sections, startMs, endMs);
			armed = false;
			setView(startMs, endMs);
			const nudged = pendingNudge;
			pendingNudge = null;
			if (nudged) {
				setTier(nudged.tier, true);
				park(startMs + nudged.tod);
			}
		}
		inspectorFocus = 'row';
		litKey = '';
		lastFix = all.filter((f) => f.t < startMs).at(-1) ?? toFixes(window.before ? [window.before] : [])[0] ?? null;
		status = fixes.length ? 'shown' : 'empty';
		drawn++;
		if (fixes.length) return;
		// No fix all day: say so, then when location was last measured (and
		// at which stay, when the last fix was in one), then what the phone did
		// that day - told only when the day's voice and steps loaded. The
		// camera holds the last fix (drawDay).
		note = quietDay(lastFix?.t ?? null, null, zone, { talk: voice.some((v) => v.speaker_count >= 2), steps: lw.steps.length > 0 });
	}

	/** The loaded day on the map: its track, then the frame, the bubbles and
	 *  the playhead; a day with no fix holds the last position. */
	async function drawDay() {
		const m = map;
		if (!m) return;
		const mine = asked;
		const { runs, bridges } = splitTrack(dayTrack);
		(m.getSource('track-run') as GeoJSONSource).setData(lines(runs));
		(m.getSource('track-bridge') as GeoJSONSource).setData(lines(bridges));
		const drives = sections
			.filter((s) => s.kind === 'transit')
			.flatMap((d) => splitTrack(dayTrack.filter((f) => f.t >= d.s && f.t < d.e)).runs);
		(m.getSource('track-drive') as GeoJSONSource).setData(lines(drives));
		// The inspector draws first, so the framing can leave room for it.
		await tick();
		if (mine !== asked) return;
		if (dayFixes.length) {
			// The first day jumps into place; every day after it glides there,
			// so a step between days reads as one move (the owner's call,
			// 2026-10-10).
			frameDay(m, framedOnce ? DAY_GLIDE_MS : 0);
			framedOnce = true;
			bubbles?.build(mapMoments());
			syncPlayhead();
			// A card opened while the day loaded (the tile's step to another
			// day): the place it asks about, not the whole day. Only a card on
			// this day: one left open on the day before must not hold the map.
			if (focused && focusedDay === date) focusPlace(focused);
			// A name opened from "Named Places" while its day loaded: its
			// question opened once the day is framed, so the framing doesn't
			// undo it.
			if (pendingShow) {
				const id = pendingShow;
				pendingShow = null;
				inspector?.revisit(id);
			}
			return;
		}
		bubbles?.clear();
		syncPlayhead();
		// The prototype's follow zoom is 14 (dayback/src/main.js:938).
		if (lastFix) m.jumpTo({ center: [lastFix.lng, lastFix.lat], zoom: 14 });
		else m.jumpTo({ center: [0, 20], zoom: 1.5 });
		// A day with no fix opens on the held position; that is its home.
		recordHome();
	}

	/** The prototype's framing padding (`v4FitPad`, dayback/src/main.js:1190):
	 *  the measured top cards, inspector and scrubber plus a buffer of max(34 px,
	 *  6 % of the smaller side) on every edge, each capped so fitBounds can
	 *  still move. */
	function fitPad(m: MlMap) {
		const c = m.getContainer();
		const cw = c.clientWidth || 900;
		const ch = c.clientHeight || 600;
		const buf = Math.max(34, Math.round(Math.min(cw, ch) * 0.06));
		const inspectorW = narrow ? 0 : (inspector?.width() ?? 0);
		return {
			top: Math.min(topChrome + buf, ch * 0.34),
			bottom: Math.min(scrubBottom() + buf, ch * (narrow ? 0.7 : 0.45)),
			left: Math.min(buf, cw * 0.4),
			right: Math.min((inspectorW ? inspectorW + 16 : 0) + buf, cw * 0.5),
		};
	}

	/** Frame the whole day's fixes (`v4Fit`, main.js:1185-1189). */
	function frameDay(m: MlMap, duration: number) {
		const ml = maplibre;
		if (!ml || !dayFixes.length) return;
		const b = new ml.LngLatBounds();
		for (const f of dayFixes) b.extend([f.lng, f.lat]);
		m.fitBounds(b, { padding: fitPad(m), maxZoom: 15, duration });
		// This camera is home: Reset view appears once you leave it.
		if (duration) m.once('moveend', recordHome);
		else recordHome();
	}

	/** Two fingers pan the map the way they move; a pinch (the browser sends
	 *  it as a wheel with ctrl) is left to MapLibre, which zooms about the
	 *  cursor; over a place card the card's list scrolls (main.js:988-992). */
	/** How fast a pinch (or a wheel with Ctrl) zooms the map. */
	const PINCH_RATE = { trackpad: 1 / 50, wheel: 1 / 360 };
	/** Two fingers pan; a mouse's wheel, with no pinch to zoom by, zooms
	 *  (MapLibre's scroll zoom, about the pointer). */
	const wheelFrom = wheelSource();
	function onWheel(e: WheelEvent) {
		following = false;
		if (e.ctrlKey || e.metaKey) return;
		if (wheelFrom(e) === 'mouse') return;
		if ((e.target as HTMLElement | null)?.closest?.('.tl-card')) return;
		e.preventDefault();
		e.stopPropagation();
		const k = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? window.innerHeight : 1;
		map?.panBy([e.deltaX * k, e.deltaY * k], { duration: 0 });
	}

	/** The day's opening camera, and whether you have left it (main.js:1196-1205):
	 *  zoomed by more than 0.12 or moved more than 40 px. */
	let home: { c: LngLat; z: number } | null = null;
	let away = $state(false);
	function recordHome() {
		if (!map) return;
		home = { c: map.getCenter(), z: map.getZoom() };
		syncAway();
	}
	function syncAway() {
		const m = map;
		if (!m || !home) {
			away = false;
			return;
		}
		const a = m.project(home.c);
		const b = m.project(m.getCenter());
		away = Math.abs(m.getZoom() - home.z) > 0.12 || Math.hypot(a.x - b.x, a.y - b.y) > 40;
	}
	/** Glide back to the day's frame; an opened knot folds into its chip. */
	function resetView() {
		following = false;
		const m = map;
		if (!m) return;
		bubbles?.fold();
		if (dayFixes.length) frameDay(m, 650);
		else if (home) m.easeTo({ center: home.c, zoom: home.z, duration: 650 });
	}
	let glow: Marker | null = null;
	/** "Help Virtues out": the questions, answers and Undo, shared by the
	 *  day's list and its Review. An answer re-reads the day's titles. */
	const review = new ReviewState(() => void refreshTitles());
	// Home and Work come first, for everyone: while either is unanswered,
	// opening the Timeline opens its question (the owner's call, 2026-10-09).
	onMount(() => {
		void review.load().then(() => {
			const first = steps[0];
			if (first && (first.ask === 'home' || first.ask === 'work')) stepTo(0);
		});
	});
	/** The places "Help Virtues out" asks about: titled "Unknown" while only
	 *  their city names them. */
	function markAsked(places: DerivedPlace[]) {
		for (const p of places) p.asked = review.byPlace.has(p.id);
	}
	$effect(() => {
		void review.byPlace;
		untrack(() => {
			if (!loaded) return;
			markAsked(loaded.derived.places);
			redrawPlaces();
		});
	});

	/** The tile's steps between days: the questions that have a day to open,
	 *  in Review's order (Home and Work, then the most time first). */
	const steps = $derived(review.questions.filter((q) => q.best_day));
	let stepAt = $state<number | null>(null);
	/** The question stepped to. Once answered it leaves the list, and the
	 *  step sits between its neighbours: the next one has taken its index. */
	let stepId = $state<string | null>(null);
	const stepAnswered = $derived(stepAt !== null && steps[stepAt]?.id !== stepId);
	/** A question whose day is loading: its card opens once the day is in. */
	let pendingAsk = $state<string | null>(null);
	function stepTo(i: number) {
		const q = steps[i];
		if (!q?.best_day) return;
		stepAt = i;
		stepId = q.id;
		if (q.best_day === date) inspector?.ask(q.id);
		else {
			pendingAsk = q.id;
			date = q.best_day;
		}
	}
	$effect(() => {
		const id = pendingAsk;
		if (!id || !sections.some((x) => x.placeId === id)) return;
		untrack(() => {
			pendingAsk = null;
			inspector?.ask(id);
		});
	});
	/** The day "Named Places" was opened on: leaving it by "‹ Back to the
	 *  Day" returns there, however many names' days it showed. */
	let detourFrom: string | null = null;
	function detour(open: boolean, back = false) {
		if (open) {
			detourFrom ??= date;
			return;
		}
		const from = detourFrom;
		detourFrom = null;
		if (back && from && from !== date) date = from;
	}
	/** The list on show: behind it, the day it was opened on, not the last
	 *  name's (the owner's call, 2026-10-13: a confirmed name left him on
	 *  its day). */
	function listShown() {
		detourFrom ??= date;
		if (date !== detourFrom) date = detourFrom;
	}
	/** A name from "Named Places": its last day, with its question asked
	 *  again under its stay once the day is drawn (`drawDay`). */
	let pendingShow: string | null = null;
	function openPlace(id: string, day: string) {
		if (day === date) inspector?.revisit(id);
		else {
			pendingShow = id;
			date = day;
		}
	}
	// The pill is there while something is left to ask. The answer that
	// clears the list leaves "All caught up" on that day, then the pill goes
	// (the owner's call, 2026-10-13: a pill that stays is noise); every name
	// stays under "Named Places" at the end of the day's list.
	let caughtUpOn = $state<string | null>(null);
	let lastCount = -1;
	$effect(() => {
		const n = steps.length;
		untrack(() => {
			if (lastCount > 0 && n === 0) caughtUpOn = date;
			lastCount = n;
		});
	});
	// An answer shortens the list: the step stays where the answered one was,
	// so the next one in line is one tap away, never skipped.
	$effect(() => {
		const n = steps.length;
		untrack(() => {
			if (stepAt !== null && stepAt > n) stepAt = n;
			if (!n) stepAt = null;
		});
	});
	/** A stay's name field made its place Home or Work. */
	async function setRole(placeId: string, role: 'home' | 'work'): Promise<boolean> {
		try {
			await sendAnswer(placeId, { kind: role });
			await Promise.all([refreshTitles(), review.load()]);
			return true;
		} catch (e) {
			console.warn('[Timeline] setting Home or Work failed', e);
			return false;
		}
	}
	/** Pointing to Home or Work on the map, after a "no": a mode of its own,
	 *  a banner at the top and one pin that each tap moves. */
	const pointing = $derived(review.pointing?.role ?? null);
	let tapped = $state<{ lat: number; lon: number } | null>(null);
	let tapMark: Marker | null = null;
	$effect(() => {
		const role = pointing;
		untrack(() => {
			tapped = null;
			tapMark?.remove();
			tapMark = null;
			const m = map;
			if (m) m.getCanvas().style.cursor = role ? 'crosshair' : '';
			if (role) focusPlace(null);
		});
	});
	function tapAt(lat: number, lon: number) {
		const m = map;
		if (!m || !maplibre) return;
		if (!tapMark) {
			const el = document.createElement('div');
			el.className = 'tl-tap';
			el.append(Object.assign(document.createElement('span'), { className: 'tl-tap-pin' }));
			tapMark = new maplibre.Marker({ element: el, anchor: 'bottom' });
		}
		tapMark.setLngLat([lon, lat]).addTo(m);
		tapped = { lat, lon };
	}
	/** How close the map comes to a place being asked about. */
	const QUESTION_ZOOM = 16.75;
	/** A place is being asked about: the map is on it, the day's marks dimmed. */
	let focusing = $state(false);
	/** Take the map to the place "Help Virtues out" is asking about, under a
	 *  glowing pin, with the day's marks dimmed (main.js:2209-2221); `null`
	 *  gives the day back. The map frames the stay picked for the question on
	 *  its own path, as a pick does, and the path keeps its weight: the
	 *  prototype faded it and framed the named places around, which left the
	 *  stay small and its path a ghost (the owner's call, 2026-10-09). */
	let focused: { lat: number; lon: number; reachM: number; shown: PlaceCandidate[] } | null = null;
	/** The day the focused question was opened on. */
	let focusedDay: string | null = null;
	/** Whether a day has been framed yet: the first jumps, the rest glide. */
	let framedOnce = false;
	const DAY_GLIDE_MS = 900;
	function focusPlace(at: { lat: number; lon: number; reachM: number; shown: PlaceCandidate[] } | null) {
		focused = at;
		focusedDay = at ? date : null;
		const m = map;
		if (!m || !maplibre || !m.getLayer('track-run')) return;
		glow?.remove();
		glow = null;
		const names = m.getSource('review-names') as GeoJSONSource | undefined;
		focusing = !!at;
		names?.setData({
			type: 'FeatureCollection',
			features: (at?.shown ?? []).map((c) => ({ type: 'Feature', properties: { name: c.name }, geometry: { type: 'Point', coordinates: [c.lon, c.lat] } })),
		});
		if (!at) return;
		const el = document.createElement('div');
		el.className = 'tl-glow';
		glow = new maplibre.Marker({ element: el, anchor: 'center' }).setLngLat([at.lon, at.lat]).addTo(m);
		// The place in the middle of the clear map, at block scale: its
		// streets named and the path around it in view (the owner's call,
		// 2026-10-10, over framing the stay's own path, which set the pin low
		// and close). An offset, not `padding`: MapLibre keeps an ease's
		// padding on the map, and every fit after it then added its own on
		// top and found no room, so "Show the whole day" did nothing.
		const lim = clearArea(m);
		const c = m.getContainer();
		const offset: [number, number] = [(lim.l + lim.r) / 2 - c.clientWidth / 2, (lim.t + lim.b) / 2 - c.clientHeight / 2];
		m.easeTo({ center: [at.lon, at.lat], zoom: QUESTION_ZOOM, offset, duration: 820 });
	}

	/** After an answer, the day's titles again: a place just named, or one no
	 *  longer called by the map's wrong match, reads its new title. */
	async function refreshTitles() {
		const at = date;
		try {
			const day = await fetchDay(at);
			if (at !== date || !loaded) return;
			for (const p of loaded.derived.places) {
				const stay = day.stays.find((s) => s.place_id === p.id);
				p.title = day.place_titles[p.id]?.title ?? p.title;
				if (stay) [p.is_named, p.place_name] = [stay.place_is_named, stay.place_name];
			}
			redrawPlaces();
		} catch (e) {
			console.warn('[Timeline] titles after an answer unavailable', e);
		}
	}

	/** The map follows the scrub (patch item 9, the owner's idea): while you
	 *  scrub, play or step with the keys, the camera keeps your pin centred in
	 *  the clear map and zooms with your pace, close in while walking around
	 *  a place, far out on a drive or a flight. The zoom comes from how far
	 *  the path covers in the five minutes either side of the playhead, eases
	 *  toward it, and only retargets on a clear change of pace. In a stretch
	 *  with no location the map shows the whole day, and picks the pin up
	 *  again where the record resumes. A pan or pinch by hand, a pick, or
	 *  "Show the whole day" stops following until the next scrub or play. */
	let following = false;
	let followTarget: number | null = null;
	let followGap = false;
	let followRaf = 0;
	const FOLLOW_WINDOW = 5 * MIN;
	const FOLLOW_MAX_ZOOM = 18;
	const FOLLOW_EASE = 0.15;
	function paceZoom(m: MlMap, t: number): number {
		const near = dayTrack.filter((f) => Math.abs(f.t - t) <= FOLLOW_WINDOW);
		const ml = maplibre;
		if (!ml || near.length < 2) return FOLLOW_MAX_ZOOM;
		const b = new ml.LngLatBounds();
		for (const f of near) b.extend([f.lng, f.lat]);
		const cam = m.cameraForBounds(b, { padding: fitPad(m), maxZoom: FOLLOW_MAX_ZOOM });
		return Math.max(3, Math.min(FOLLOW_MAX_ZOOM, cam?.zoom ?? FOLLOW_MAX_ZOOM));
	}
	function followTick(m: MlMap) {
		cancelAnimationFrame(followRaf);
		const at = positionAt(dayTrack, playT);
		if (!at) {
			if (!followGap) {
				followGap = true;
				followTarget = null;
				frameDay(m, 650);
			}
			return;
		}
		followGap = false;
		const target = paceZoom(m, playT);
		if (followTarget === null || Math.abs(target - followTarget) > 0.75) followTarget = target;
		const z = m.getZoom();
		const zoom = Math.abs(followTarget - z) < 0.02 ? followTarget : z + (followTarget - z) * FOLLOW_EASE;
		const lim = clearArea(m);
		const c = m.getContainer();
		const offset: [number, number] = [(lim.l + lim.r) / 2 - c.clientWidth / 2, (lim.t + lim.b) / 2 - c.clientHeight / 2];
		m.easeTo({ center: [at.lng, at.lat], zoom, offset, duration: 0 });
		// The zoom settles on its own once the playhead rests.
		if (zoom !== followTarget) followRaf = requestAnimationFrame(() => following && followTick(m));
	}

	/** The keys (main.js:792-802, 1205): Esc is Reset view; ← → nudge the
	 *  playhead; Space plays. Shift+← → do nothing (the owner's call: the
	 *  prototype's jump between conversations is left out). Only while the
	 *  Timeline is on screen, and never while typing. */
	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			if (pointing) {
				e.preventDefault();
				review.cancelPointing();
			} else if (monthOpen) {
				e.preventDefault();
				monthOpen = false;
			} else if (picked) {
				// Esc lets go of a picked drive or stay first.
				e.preventDefault();
				unpick();
			} else if (away && !e.defaultPrevented) resetView();
			return;
		}
		if (!root?.getClientRects().length || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
		const el = e.target as HTMLElement | null;
		const tag = el?.tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el?.isContentEditable) return;
		if ((e.key === 'ArrowRight' || e.key === 'ArrowLeft') && !e.shiftKey) {
			e.preventDefault();
			nudge(e.key === 'ArrowRight' ? 1 : -1);
		} else if (e.code === 'Space' && tag !== 'BUTTON') {
			e.preventDefault();
			togglePlay();
		}
	}

	/** The day's conversations and walks at their anchors, each moving when it
	 *  covered over half a kilometre of track (main.js:1028-1039). */
	function mapMoments(): MapMoment[] {
		const out: MapMoment[] = [];
		for (const r of sections.flatMap((s) => s.rows)) {
			const at = anchorAt(dayTrack, r.s, r.e);
			if (!at) continue;
			const km = trackMetres(dayTrack, r.s, r.e) / 1000;
			const flight = sections.some((x) => x.kind === 'transit' && x.flight && x.s <= r.s && r.s < x.e);
			out.push({ s: r.s, e: r.e, kind: r.kind, title: r.title, lng: at.lng, lat: at.lat, km, moving: km > 0.5, flight });
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

	/** Whether the scrubber shows more than the day: the Week span, or a
	 *  pinch out past it. */
	const wide = (a = viewStart, b = viewEnd) => b - a > dayEnd - dayStart + MIN;
	/** Where the playhead and the view may go: the day, or on the Week span
	 *  the week, never past today. */
	function bounds(isWide: boolean): [number, number] {
		return isWide ? [weekStart, Math.min(weekEnd, localDay(today).endMs)] : [dayStart, dayEnd];
	}

	/** Move the playhead: a pick lands exactly there, a hair before the end at
	 *  most (`v4Seek`, main.js:491). Inside the day, everything follows it; on
	 *  the Week span it may walk into another day, and that day loads around
	 *  it (the owner's call; the prototype's playhead never left its day). */
	function park(t: number) {
		unpick();
		armed = true;
		const [lo, hi] = bounds(wide());
		playT = Math.max(lo, Math.min(hi - 1000, t));
		if (playT < dayStart || playT >= dayEnd) {
			follow(playT);
			return;
		}
		ensureVisible();
		syncPlayhead();
	}
	/** When the day began for the person: the end of the night they woke
	 *  from inside the day, else the start of its first stay or drive after
	 *  midnight, else midnight. */
	function wakeOf(secs: InspectorSection[], startMs: number, endMs: number): number {
		const woke = secs.filter((x) => x.kind === 'sleep' && x.e > startMs && x.e < endMs).sort((a, b) => a.e - b.e)[0];
		if (woke) return woke.e;
		const first = secs.find((x) => (x.kind === 'place' || x.kind === 'transit') && x.s > startMs && x.s < endMs);
		return first?.s ?? startMs;
	}

	/** Load the day holding `t`, keeping the playhead and the week. */
	function follow(t: number) {
		const slug = new Date(t).toLocaleDateString('en-CA', { timeZone: zone });
		if (slug === date) return;
		crossing = true;
		date = slug;
	}

	/** The view never pans past the day (the owner's call); wider than the
	 *  day, it never pans past the week, and is centred on it when wider still
	 *  (the prototype let it run to two days either side of the record). */
	function setView(a: number, b: number) {
		const [lo, hi] = wide(a, b) ? [weekStart, weekEnd] : [dayStart, dayEnd];
		[viewStart, viewEnd] = clampView(a, b, lo, hi);
	}
	/** The view scrolls only when the playhead would leave it, landing it a
	 *  tenth in from the edge it crossed (main.js:492-495). */
	function ensureVisible() {
		if (playT >= viewStart && playT <= viewEnd) return;
		const span = viewEnd - viewStart;
		const a = playT < viewStart ? playT - span * 0.1 : playT + span * 0.1 - span;
		setView(a, a + span);
	}

	// An indirect move travels (main.js:503-513): a tier, a jump. It glides in
	// warped time, so it reads linear on screen, fast out of the gate with a
	// long tail. A direct one (a drag, a pinch) is instant and cancels it.
	let anim = 0;
	function cancelAnim() {
		if (anim) cancelAnimationFrame(anim);
		anim = 0;
	}
	function glideView(a: number, b: number, ms = 520) {
		cancelAnim();
		const wa0 = warp(folds, viewStart);
		const wb0 = warp(folds, viewEnd);
		const wa1 = warp(folds, a);
		const wb1 = warp(folds, b);
		const t0 = performance.now();
		const step = (now: number) => {
			const p = Math.min(1, (now - t0) / ms);
			const e = 1 - Math.pow(1 - p, 4);
			setView(unwarp(folds, wa0 + (wa1 - wa0) * e), unwarp(folds, wb0 + (wb1 - wb0) * e));
			anim = p < 1 ? requestAnimationFrame(step) : 0;
		};
		anim = requestAnimationFrame(step);
	}
	/** Week, Day, Hour, Minute (main.js:82-88): the week around the day, the
	 *  day, three hours or fourteen minutes around the playhead. */
	function setTier(t: Tier, instant = false) {
		const [a, b] = tierView(t, { s: dayStart, e: dayEnd }, { s: weekStart, e: weekEnd }, playT);
		if (instant) setView(a, b);
		else glideView(a, b, 560);
	}
	/** A pinch zooms the scale around the playhead, keeping it on the same
	 *  spot, no wider than 40 hours; the Week tier goes wider (main.js:496-500). */
	function zoom(factor: number) {
		cancelAnim();
		const f = Math.min(Math.max(factor, 0.5), 2);
		const wvs = warp(folds, viewStart);
		const span = warp(folds, viewEnd) - wvs;
		const wp = warp(folds, playT);
		const frac = span ? (wp - wvs) / span : 0.5;
		const ns = span * f;
		let vs = unwarp(folds, wp - frac * ns);
		let ve = unwarp(folds, wp + (1 - frac) * ns);
		if (ve - vs > 40 * HOUR) {
			const c = (vs + ve) / 2;
			vs = c - 20 * HOUR;
			ve = c + 20 * HOUR;
		}
		setView(vs, ve);
	}
	/** A click, a drag or a sideways swipe on the scrubber: the playhead goes
	 *  exactly there (main.js:501, 1409-1420). */
	function scrubSeek(t: number) {
		cancelAnim();
		inspectorFocus = 'row';
		following = true;
		park(t);
	}

	/** A sideways swipe moves the playhead through time. When the view shows
	 *  less than the whole day (Hour, Minute), the view slides with it, so the
	 *  playhead holds its place on screen and the hours pass under it (the
	 *  owner's call; the prototype's playhead ran off the edge and the view
	 *  jumped after it). */
	function swipe(t: number) {
		cancelAnim();
		inspectorFocus = 'row';
		following = true;
		if (viewStart <= dayStart && viewEnd >= dayEnd) {
			park(t);
			return;
		}
		const wvs = warp(folds, viewStart);
		const span = warp(folds, viewEnd) - wvs;
		const frac = span ? (warp(folds, playT) - wvs) / span : 0.5;
		park(t);
		const wp = warp(folds, playT);
		setView(unwarp(folds, wp - frac * span), unwarp(folds, wp + (1 - frac) * span));
	}

	// Play runs the day at 300x, five minutes a second, in real time whatever
	// the zoom; it stops at the day's end, and play at the end starts over
	// (main.js:580-597).
	const PLAY_RATE = 300;
	let loopRaf = 0;
	let loopTs: number | null = null;
	function togglePlay() {
		playing = !playing;
		if (!playing) {
			cancelAnimationFrame(loopRaf);
			loopRaf = 0;
			loopTs = null;
			return;
		}
		// Play is a touch too, and the map follows it.
		armed = true;
		following = true;
		inspectorFocus = 'row';
		if (playT >= dayEnd - 2 * MIN) playT = dayStart;
		loopRaf = requestAnimationFrame(loop);
	}
	function loop(now: number) {
		if (!playing) {
			loopRaf = 0;
			loopTs = null;
			return;
		}
		if (loopTs === null) {
			loopTs = now;
			loopRaf = requestAnimationFrame(loop);
			return;
		}
		const dt = Math.min(100, now - loopTs);
		loopTs = now;
		const next = playT + dt * PLAY_RATE;
		// On the Week span play runs on into the next day, which loads around it.
		const [, end] = bounds(wide());
		if (next >= end) {
			playing = false;
			loopRaf = 0;
			loopTs = null;
			return;
		}
		playT = next;
		if (next >= dayEnd) follow(next);
		else {
			ensureVisible();
			syncPlayhead();
		}
		loopRaf = requestAnimationFrame(loop);
	}

	/** ← → move the playhead one unit of the lit tier (main.js:1383-1386): a
	 *  minute or an hour inside the day; a day or a week crosses to that day,
	 *  keeping the time of day and the tier, never past today. */
	function nudge(dir: 1 | -1) {
		following = true;
		const t = tier;
		if (t === 'min' || t === 'hour') {
			inspectorFocus = 'row';
			park(playT + dir * (t === 'min' ? MIN : HOUR));
			return;
		}
		let next = stepDay(date, dir * (t === 'week' ? 7 : 1));
		if (next > today) next = today;
		if (next === date) return;
		pendingNudge = { tier: t, tod: playT - dayStart };
		date = next;
	}


	// The week's lanes load the first time the view leaves the day: the Week
	// span, or a pinch out past midnight.
	let wideAsked = 0;
	$effect(() => {
		if (!lanes.b || (viewStart >= lanes.a && viewEnd <= lanes.b)) return;
		untrack(() => void loadWide());
	});
	async function loadWide() {
		const mine = asked;
		if (wideAsked === mine) return;
		wideAsked = mine;
		const b = { startMs: weekStart, endMs: weekEnd, zone };
		const slugs = Array.from({ length: 7 }, (_, i) => stepDay(weekSlug, i));
		const days = await Promise.all(
			slugs.map((d) =>
				fetchDay(d).catch((e) => {
					console.warn('[Timeline] week day fetch failed', d, e);
					return null;
				}),
			),
		);
		if (mine !== asked) return;
		const { derived, voice, lanes: lw } = joinDays(days.flatMap((d) => (d ? [deriveDay(d, sources)] : [])));
		lanes = laneData(derived, voice, lw, b.startMs, b.endMs);
	}

	/** Everything that follows the playhead on the map (main.js:1004-1016,
	 *  1103-1113): the pin, the lit bubble, the lit stretch of path - the
	 *  drive under the playhead, else the moving moment under it. The drive
	 *  comes first (the prototype put the moment first), so a conversation
	 *  that runs from a stay into a drive never lights its whole path over
	 *  the drive, and a drive picked from the inspector lights and glows itself. */
	function syncPlayhead() {
		const m = map;
		if (!m) return;
		if (following) followTick(m);
		if (picked && !(picked.s <= playT && playT < picked.e)) unpick();
		const at = positionAt(dayTrack, playT);
		// A picked stay's pulse is where you were; the pin waits until the
		// playhead leaves it, so one mark says "here".
		if (at && pin && !pulse) pin.setLngLat([at.lng, at.lat]).addTo(m);
		else pin?.remove();
		bubbles?.sync(playT);
		const drive = sections.find((s) => s.kind === 'transit' && s.s <= playT && playT < s.e);
		const moving = drive ? null : (bubbles?.movingCurrent() ?? null);
		const seg = drive ?? moving ?? null;
		const key = drive ? `t${drive.s}` : moving ? `m${moving.s}` : '';
		if (key === litKey) return;
		litKey = key;
		const pts = seg ? dayTrack.filter((f) => f.t >= seg.s && f.t < seg.e) : [];
		const { runs, bridges } = splitTrack(pts);
		(m.getSource('lit-run') as GeoJSONSource | undefined)?.setData(pts.length >= 2 ? lines(runs) : lines([]));
		(m.getSource('lit-bridge') as GeoJSONSource | undefined)?.setData(pts.length >= 2 ? lines(bridges) : lines([]));
	}

	/** An inspector pick parks the playhead and takes the map there (main.js:1634-1637,
	 *  1087-1101): a row reveals its bubble; a section parks at its start (not
	 *  when it is already the live one) and a drive is framed whole, anything
	 *  else opens its bubbles' chip or pans to where the track says you were. */
	/** A click on a stretch of the scrubber's Location lane: the playhead is
	 *  already where the click landed; the map goes to the stretch holding it,
	 *  as a pick in the inspector would take it (the owner's call). */
	/** The stay or drive under the pointer on the map: its stretch of path
	 *  drawn heavier, and its row in the list lit as on hover, so you see what
	 *  a click will pick. */
	let hovered = $state<{ kind: 'place' | 'transit'; s: number } | null>(null);
	let hoverFrame = 0;
	function hoverPath(m: MlMap, at: { x: number; y: number } | null) {
		cancelAnimationFrame(hoverFrame);
		hoverFrame = requestAnimationFrame(() => {
			const sec = at && !pointing && !focusing ? sectionAt(m, at) : null;
			const next = sec && !(picked && picked.s === sec.s) ? { kind: sec.kind as 'place' | 'transit', s: sec.s } : null;
			if (next?.s === hovered?.s && next?.kind === hovered?.kind) return;
			hovered = next;
			const src = m.getSource('hover-run') as GeoJSONSource | undefined;
			if (!src) return;
			if (!sec || !next) return src.setData(lines([]));
			m.setPaintProperty('hover-run', 'line-color', next.kind === 'transit' ? ink.move : ink.track);
			src.setData(lines(splitTrack(dayTrack.filter((f) => f.t >= sec.s && f.t < sec.e)).runs));
		});
	}
	/** The layers a click on the day's path lands on. */
	const PATH_HIT = ['track-hit', 'drive-hit'];
	/** The stay or drive a click on the path belongs to: the time of the
	 *  path's nearest point to it, and the section holding that time. */
	function sectionAt(m: MlMap, at: { x: number; y: number }): InspectorSection | null {
		let best: Fix | null = null;
		let bestD = Infinity;
		for (const f of dayTrack) {
			const p = m.project([f.lng, f.lat]);
			const d = Math.hypot(p.x - at.x, p.y - at.y);
			if (d < bestD) [best, bestD] = [f, d];
		}
		if (!best) return null;
		const t = best.t;
		return sections.find((s) => (s.kind === 'place' || s.kind === 'transit') && s.s <= t && t < s.e) ?? null;
	}

	function revealAt(kind: RibbonKind, t: number) {
		const sec = sections.find((s) => s.kind === kind && s.s <= t && t < s.e);
		if (sec) reveal({ kind: sec.kind, s: sec.s, e: sec.e }, false);
	}

	function reveal(p: InspectorPick, parkIt = true) {
		following = false;
		const m = map;
		const ml = maplibre;
		if (!m || !ml) return;
		if (p.kind === 'conversation' || p.kind === 'walk') {
			pickMoment(p.s, false);
			return;
		}
		// A second click on the picked drive or stay undoes the first: it
		// lets go of it and the map goes back to the whole day (the owner's
		// call, 2026-10-08).
		if (picked && picked.s === p.s && picked.kind === (p.kind === 'transit' ? 'transit' : 'place')) {
			unpick();
			resetView();
			return;
		}
		const i = sections.findIndex((s) => s.kind === p.kind && s.s === p.s);
		const live = sections.findLastIndex((s) => s.s <= playT && playT < s.e);
		inspectorFocus = 'sec';
		if (parkIt && i !== live) park(p.s);
		if (p.kind === 'transit') {
			bubbles?.fold();
			const path = dayTrack.filter((f) => f.t >= p.s && f.t < p.e);
			if (path.length >= 2) {
				const b = new ml.LngLatBounds();
				for (const f of path) b.extend([f.lng, f.lat]);
				m.fitBounds(b, { padding: fitPad(m), maxZoom: 15, duration: 650 });
				pick({ kind: 'transit', s: p.s, e: p.e });
				return;
			}
		}
		// A stay flies in to its place, close enough to read the streets around
		// it, and its place pulses while it stays picked (the owner's call; the
		// prototype only pans a stay onto the map, main.js:1101).
		const placeId = sections[i]?.placeId ?? '';
		const place = p.kind === 'place' ? placeCoords.get(placeId) : undefined;
		if (place) {
			bubbles?.fold();
			frameStay(m, place, p.s, p.e);
			pick({ kind: 'place', s: p.s, e: p.e, at: place, asked: review.byPlace.has(placeId) });
			return;
		}
		const keys = sections[i]?.rows.map((r) => r.s) ?? [];
		if (bubbles?.revealSection(keys)) return;
		const at = anchorAt(dayTrack, p.s, p.e);
		if (at) showPoint(m, [at.lng, at.lat]);
	}

	/** A conversation or a walk picked anywhere - its inspector row, its bubble, a
	 *  place card's row - does one thing: the playhead goes to its start, its
	 *  row opens in the inspector (a click on the inspector's own row opens or closes it
	 *  there), and the map flies in to it, or opens the chip holding it (the
	 *  owner's call; the prototype only panned a bubble into view). */
	function pickMoment(s: number, openRow: boolean) {
		following = false;
		inspectorFocus = 'row';
		park(s);
		if (openRow) inspector?.expand(s);
		// Above the ground, where a conversation happened says nothing: a
		// moment on a flight frames the flight, as picking the flight does
		// (the owner's call, 2026-10-08).
		const flight = sections.find((x) => x.kind === 'transit' && x.flight && x.s <= s && s < x.e);
		const m = map;
		const ml = maplibre;
		const path = flight ? dayTrack.filter((f) => f.t >= flight.s && f.t < flight.e) : [];
		if (m && ml && path.length >= 2) {
			bubbles?.fold();
			const b = new ml.LngLatBounds();
			for (const f of path) b.extend([f.lng, f.lat]);
			m.fitBounds(b, { padding: fitPad(m), maxZoom: 15, duration: 650 });
			return;
		}
		bubbles?.reveal(s);
	}

	/** Fly in to one spot, centred on the clear map, to zoom 16 - streets and
	 *  buildings - never zooming out. An indirect move, so it travels. */
	function flyIn(m: MlMap, at: [number, number]) {
		const lim = clearArea(m);
		const c = m.getContainer();
		const offset: [number, number] = [(lim.l + lim.r) / 2 - c.clientWidth / 2, (lim.t + lim.b) / 2 - c.clientHeight / 2];
		m.easeTo({ center: at, zoom: Math.max(m.getZoom(), 16), offset, duration: 650 });
	}

	/** A picked stay frames its own stretch of path, as close as it takes to
	 *  show the walking inside it (a terminal, a bar's floor) up to zoom 18.5;
	 *  a stay that barely moved, or whose path spreads wider than streets,
	 *  flies in to street level as before (the owner's call, 2026-10-08). */
	const STAY_MAX_ZOOM = 18.5;
	function frameStay(m: MlMap, place: [number, number], s: number, e: number) {
		const ml = maplibre;
		const path = dayTrack.filter((f) => f.t >= s && f.t < e);
		if (!ml || path.length < 2) return flyIn(m, place);
		const b = new ml.LngLatBounds(place, place);
		for (const f of path) b.extend([f.lng, f.lat]);
		const cam = m.cameraForBounds(b, { padding: fitPad(m), maxZoom: STAY_MAX_ZOOM });
		if (!cam || (cam.zoom ?? 0) < 16) return flyIn(m, place);
		m.fitBounds(b, { padding: fitPad(m), maxZoom: STAY_MAX_ZOOM, duration: 650 });
	}

	/** A stay or a drive picked from the inspector stays marked while the playhead
	 *  is inside it: a stay's place wears a ring, a drive's lit stretch a halo.
	 *  Any other pick, a scrub, or the playhead leaving it clears the mark. */
	let picked: { kind: 'place' | 'transit'; s: number; e: number } | null = null;
	/** The picked stay or drive, for the list's selection; kept apart from
	 *  `picked`, which the playhead's own code reads and clears. */
	let selected = $state<{ kind: 'place' | 'transit'; s: number } | null>(null);
	let pulse: Marker | null = null;
	function pick(p: { kind: 'place' | 'transit'; s: number; e: number; at?: [number, number]; asked?: boolean }) {
		unpick();
		const m = map;
		const ml = maplibre;
		if (!m || !ml) return;
		picked = { kind: p.kind, s: p.s, e: p.e };
		selected = { kind: p.kind, s: p.s };
		if (p.at) {
			const el = document.createElement('div');
			el.className = 'tl-pulse';
			// A stay Virtues has a question about: a "?" on its pin, and no
			// names around it until its card is open.
			if (p.asked) {
				el.classList.add('asked');
				el.textContent = '?';
			}
			pulse = new ml.Marker({ element: el, anchor: 'center' }).setLngLat(p.at).addTo(m);
			pin?.remove();
			return;
		}
		if (!m.getLayer('lit-glow')) return;
		// The picked drive stands out from a drive the playhead merely passes
		// through: twice the path's weight, over a still halo. Nothing on the
		// page breathes but Home's now-marker (design-grammar §7).
		// The picked drive turns the brighter orange (PROTOTYPE-RULES 9.21, 9.27).
		m.setPaintProperty('lit-run', 'line-width', 6);
		m.setPaintProperty('lit-bridge', 'line-width', 2.5);
		m.setPaintProperty('lit-run', 'line-color', ink.movePicked);
		m.setPaintProperty('lit-bridge', 'line-color', ink.movePicked);
		m.setPaintProperty('lit-glow', 'line-opacity', 0.3);
	}
	function unpick() {
		picked = null;
		selected = null;
		const m = map;
		if (pulse) {
			pulse.remove();
			pulse = null;
			// The pin comes back where the playhead is.
			const at = positionAt(dayTrack, playT);
			if (at && pin && m) pin.setLngLat([at.lng, at.lat]).addTo(m);
		}
		if (!m?.getLayer('lit-glow')) return;
		m.setPaintProperty('lit-glow', 'line-opacity', 0);
		m.setPaintProperty('lit-run', 'line-width', 3);
		m.setPaintProperty('lit-bridge', 'line-width', 1.5);
		m.setPaintProperty('lit-run', 'line-color', ink.move);
		m.setPaintProperty('lit-bridge', 'line-color', ink.move);
	}

	/** The clear map (`v4ClearArea`, main.js:1042-1044): not under the top
	 *  cards, the inspector or the scrubber, 8 px in from every edge. */
	function clearArea(m: MlMap): Area {
		const c = m.getContainer();
		const inspectorW = narrow ? 0 : (inspector?.width() ?? 0);
		return { l: 8, t: topChrome + 8, r: c.clientWidth - (inspectorW ? inspectorW + 20 : 0) - 8, b: c.clientHeight - scrubBottom() - 8 };
	}

	/** What the scrubber takes from the bottom: its card and 26 px under it,
	 *  220 before it has drawn (main.js:1044, 1192). */
	/** How far the bottom cards reach up the pane: the scrubber, and in the
	 *  narrow layout the sheet under it. */
	const scrubBottom = () =>
		(scrubH ? scrubH + 26 : 220) + (narrow && shownSections.length ? Math.round((root?.clientHeight ?? 0) * SHEET) + 8 : 0);

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

	// A click anywhere but the date card and its month closes the month.
	$effect(() => {
		if (!monthOpen) return;
		const close = (e: PointerEvent) => {
			if (!dateWrap?.contains(e.target as Node)) monthOpen = false;
		};
		document.addEventListener('pointerdown', close, true);
		return () => document.removeEventListener('pointerdown', close, true);
	});

	onDestroy(() => {
		clearTimeout(mapRetry);
		unpick();
		cancelAnim();
		cancelAnimationFrame(loopRaf);
		container?.removeEventListener('wheel', onWheel, { capture: true });
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

<svelte:window onkeydown={onKey} />

<div
	class="timeline"
	class:focusing={focusing || !!pointing}
	class:with-inspector={shownSections.length > 0}
	class:narrow
	bind:this={root}
	bind:clientWidth={paneW}
	style="{colourVars(dark)}; --top-chrome: {topChrome}px; --scrub-h: {scrubH}px; --inspector-top: {inspectorLow ? topChrome + 12 : 16}px"
>
	<div class="timeline-map" bind:this={container}></div>

	<!-- No header band: the map runs edge to edge and every control floats on
	     it as its own card. The date card, top left: the date, left-aligned
	     like Calendar's month title, with ‹ Today › and the month beside it. -->
	<div class="date" bind:this={dateWrap}>
		<div class="date-card tile" bind:offsetWidth={cardW} bind:offsetHeight={cardH}>
			<div class="date-row">
				<IconButton icon="mynaui:chevron-left" label="Previous day" size="sm" onclick={() => (date = stepDay(date, -1))} />
				<!-- Calendar's ‹ Today ›: a button between the arrows, there on
				     every day and greyed out on today, so it never reads as the
				     date's label and the › never moves. -->
				<span class="date-today"><Pill disabled={date === today} onclick={() => (date = today)}>Today</Pill></span>
				<IconButton icon="mynaui:chevron-right" label="Next day" size="sm" disabled={date >= today} onclick={() => (date = stepDay(date, 1))} />
				<IconButton icon="mynaui:calendar" label="Show the month" size="sm" pressed={monthOpen} onclick={() => (monthOpen = !monthOpen)} />
			</div>
			<button class="date-title" type="button" title="Back to the whole day" onclick={resetView}>{title}</button>
		</div>
		{#if review.review && !monthOpen && (steps.length || caughtUpOn === date)}
			<ReviewTile
				count={steps.length}
				at={stepAt}
				answered={stepAnswered}
				onstep={stepTo}
				onnames={() => inspector?.names()}
				onclose={() => {
					stepAt = null;
					pendingAsk = null;
				}}
			/>
		{/if}
		{#if monthOpen}
			<TimelineMonth
				{date}
				{today}
				onpick={(slug) => {
					monthOpen = false;
					date = slug;
				}}
			/>
		{/if}
	</div>

	{#if pointing}
		<!-- Pointing to Home or Work: its own mode, top centre, in the accent. -->
		<div class="pointing" role="dialog" aria-label={pointing === 'home' ? 'Tap your home' : 'Tap where you work'}>
			<p class="pointing-text">
				<Icon icon={pointing === 'home' ? 'mynaui:home' : 'mynaui:briefcase'} width="18" />
				{pointing === 'home' ? 'Tap your home on the map' : 'Tap where you work on the map'}
			</p>
			<p class="pointing-sub">{tapped ? 'Tap again to move the pin.' : 'Zoom or move the map to find it.'}</p>
			<div class="pointing-actions">
				<button class="pointing-btn" type="button" onclick={() => review.cancelPointing()}>Cancel</button>
				<button class="pointing-btn save" type="button" disabled={!tapped} onclick={() => tapped && review.savePointed(tapped)}>Save Here</button>
			</div>
			<button class="pointing-btn skip" type="button" onclick={() => review.skipPointing()}>Skip - don't ask again</button>
		</div>
	{/if}

	<TimelineInspector bind:this={inspector} sections={shownSections}
		onname={namePlace}
		onrole={setRole}
		homeSet={review.homeSet}
		{review}
		{selected}
		{hovered}
		onfocus={focusPlace}
		onmerge={mergeInto}
		{zone}
		day={date}
		{playT}
		{armed}
		focus={inspectorFocus}
		onopenplace={openPlace}
		ondetour={detour}
		onlist={listShown}
		onpick={reveal}
		onseek={(t) => {
			inspectorFocus = 'row';
			park(t);
		}}
	/>
	{#if dayStart}
		<TimelineScrubber
			bind:cardHeight={scrubH}
			{viewStart}
			{viewEnd}
			{dayStart}
			{dayEnd}
			{folds}
			{playT}
			{armed}
			{playing}
			{tier}
			{zone}
			offset={zoneOffset(dayStart, zone)}
			ribbon={lanes.ribbon}
			voice={lanes.voice}
			conversations={lanes.conversations}
			nights={lanes.nights}
			steps={lanes.steps}
			stepScale={lanes.stepScale}
			hasCalendar={lanes.hasCalendar}
			calendar={lanes.calendar}
				onseek={scrubSeek}
			onswipe={swipe}
			onribbon={(r) => revealAt(r.kind, r.t)}
			onzoom={zoom}
			ontier={(t) => setTier(t)}
			onplay={togglePlay}
		/>
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
	{#if away}
		<!-- Apple's re-centre pattern: it exists only once the camera has left the
		     day's frame (dayback/index.html:822-826, 1180). -->
		<button class="reset tile" type="button" title="Back to the whole day (Esc)" onclick={resetView}>
			Show the whole day
		</button>
	{/if}
	{#if !basemap}
		<p class="map-note tile">Your server doesn't have map tiles yet.</p>
	{:else if streetless}
		<p class="map-note tile">Your server doesn't have a street map of this area.</p>
	{/if}
</div>

<style>
	/* Every card over the map is the page's own material: the surface inside a
	   hairline at 12px, and no shadow (design-grammar §6). It reads as separate
	   because it floats over the map, not because it is lifted. */
	.timeline {
		position: absolute;
		inset: 0;
		/* What the inspector takes from the map's right edge, with its gaps. */
		--inspector-space: 16px;
		--tile-radius: var(--radius-xl);
	}
	/* The map's markers stack at z 1 to 7 (bubbles, chips, the place card);
	   everything over the map sits at 10 and up. */
	.timeline-map {
		position: absolute;
		inset: 0;
	}
	.tile {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
	}
	.timeline.with-inspector {
		--inspector-space: calc(min(384px, 42%) + 32px);
	}
	/* Narrow: the list is a sheet across the bottom third, so nothing sits
	   beside the map and the bottom cards reach further up. */
	.timeline.narrow {
		--inspector-space: 16px;
		--sheet: 34%;
	}
	.timeline.narrow.with-inspector {
		--below-map: calc(var(--scrub-h) + var(--sheet) + 8px);
	}
	.timeline.narrow .reset {
		right: 16px;
	}
	.timeline.narrow.with-inspector .reset,
	.timeline.narrow.with-inspector .map-note {
		bottom: calc(var(--below-map) + 28px);
	}
	.timeline.narrow.with-inspector .note {
		top: calc((var(--top-chrome) + 12px + 100% - var(--below-map) - 28px) / 2);
	}

	/* The date card, top left: the day's controls, then the date as the page's
	   title - the 36px serif of the scale, never bold. */
	.date {
		position: absolute;
		top: 16px;
		left: 16px;
		z-index: 12;
	}
	.date-card {
		padding: 12px 20px 16px 16px;
		border-radius: var(--tile-radius);
	}
	.date-row {
		display: flex;
		align-items: center;
		gap: 4px;
		min-height: 24px;
	}
	.date-today {
		margin: 0 2px;
	}
	.date-title {
		display: block;
		margin: 8px 0 0 4px;
		padding: 0;
		border: 0;
		background: none;
		text-align: left;
		cursor: pointer;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 36px;
		letter-spacing: -0.02em;
		line-height: 1;
		white-space: nowrap;
		color: var(--color-foreground);
	}

	/* Back to the whole day: a quiet verb in the accent, since it is pressable,
	   at the map's bottom right above the scrubber. It exists only once the
	   camera has left the day's frame. */
	.reset {
		position: absolute;
		right: calc(min(384px, 42%) + 32px);
		bottom: calc(var(--scrub-h) + 28px);
		z-index: 11;
		padding: 8px 16px;
		border-radius: var(--radius-full);
		font-family: var(--font-sans);
		font-size: 14px;
		font-weight: 500;
		/* Virtues' secondary button: the foreground's ink on the surface. */
		color: var(--color-foreground);
		cursor: pointer;
	}
	.timeline:not(.with-inspector) .reset {
		right: 16px;
	}
	.reset:hover {
		background: var(--hover-bg);
	}

	/* A day with no location: its one fact, centred on the clear map between
	   the date card and the scrubber, the left edge and the inspector. The
	   map under it holds the last known position. */
	.note {
		position: absolute;
		z-index: 10;
		top: calc((var(--top-chrome) + 12px + 100% - var(--scrub-h) - 28px) / 2);
		left: calc((16px + 100% - var(--inspector-space)) / 2);
		transform: translate(-50%, -50%);
		padding: 12px 20px;
		border-radius: var(--tile-radius);
		text-align: center;
	}
	.note p {
		margin: 0;
	}
	.note-title {
		font-family: var(--font-serif);
		font-size: 18px;
		color: var(--color-foreground);
	}
	.note-line {
		margin-top: 4px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	/* Why the map is bare, just above the scrubber at the left. */
	.map-note {
		position: absolute;
		left: 16px;
		bottom: calc(var(--scrub-h) + 28px);
		z-index: 10;
		margin: 0;
		padding: 8px 12px;
		border-radius: var(--tile-radius);
		color: var(--color-foreground-muted);
		font-size: 13px;
	}

	/* The map's own marks, made by MapLibre markers outside this component's
	   markup, so :global. The pin is now, so it takes the accent. */
	/* The place asked about: a glowing pin (index.html:321-323), still, as the
	   pane's grammar keeps motion for Home's now-marker. */
	.timeline :global(.tl-glow) {
		width: 18px;
		height: 18px;
		border-radius: 50%;
		background: var(--c-place);
		border: 3px solid var(--color-background);
		box-shadow: 0 0 0 6px color-mix(in srgb, var(--c-place) 30%, transparent); /* design-ok: the prototype's glow around the place asked about (index.html:321) */
	}
	/* The spot tapped as Home or Work: a map pin in the accent, its point on
	   the spot. MapLibre moves the marker by its own transform, so the pin's
	   shape is drawn inside it. */
	.timeline :global(.tl-tap) {
		padding-bottom: 8px;
	}
	.timeline :global(.tl-tap-pin) {
		display: block;
		width: 22px;
		height: 22px;
		border: 3px solid var(--color-background);
		border-radius: 50% 50% 50% 0;
		background: var(--color-primary);
		transform: rotate(-45deg);
	}
	/* Pointing to Home or Work: a banner of its own at the top centre, in the
	   accent, so the mode can't be mistaken for the card. */
	.pointing {
		position: absolute;
		top: 16px;
		left: 50%;
		z-index: 14;
		transform: translateX(-50%);
		width: min(360px, calc(100% - 32px));
		padding: 12px 16px;
		border-radius: var(--tile-radius);
		background: var(--color-primary);
		color: var(--color-background);
		font-family: var(--font-sans);
		text-align: center;
	}
	.pointing-text {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		margin: 0;
		font-size: 16px;
		font-weight: 600;
	}
	.pointing-sub {
		margin: 4px 0 12px;
		font-size: 13px;
		opacity: 0.85;
	}
	.pointing-actions {
		display: flex;
		gap: 8px;
	}
	/* Pills, as everywhere in the Timeline (Pill.svelte), drawn on the
	   banner's blue: Save Here filled white, Cancel and Skip quiet. */
	.pointing-btn {
		flex: 1;
		min-height: 32px;
		border: 0;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, var(--color-background) 18%, transparent);
		color: var(--color-background);
		font-family: var(--font-sans);
		font-size: 15px;
		font-weight: 500;
		cursor: pointer;
	}
	.pointing-btn.save {
		background: var(--color-background);
		color: var(--color-primary);
	}
	.pointing-btn:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.pointing-btn.skip {
		min-height: 28px;
		margin-top: 8px;
		padding: 0 12px;
		background: none;
		font-size: 12px;
	}
	.pointing-btn.skip:hover {
		background: color-mix(in srgb, var(--color-background) 18%, transparent);
	}
	/* The day's note speaks of the day, not of the place asked about or
	   pointed to. */
	.timeline.focusing .note {
		display: none;
	}
	/* The day's own marks step back while a place is asked about. MapLibre
	   writes a marker's opacity inline, so a filter fades them instead. */
	.timeline.focusing :global(.tl-pin),
	.timeline.focusing :global(.tl-bubble),
	.timeline.focusing :global(.tl-knot) {
		filter: opacity(0.22);
	}
	/* A faded mark is a mark; a faded caption is a ghost over the place
	   being asked about. Captions and knot cards wait until the card closes. */
	.timeline.focusing :global(.tl-bubble-label),
	.timeline.focusing :global(.tl-card) {
		display: none;
	}
	.timeline :global(.tl-pin) {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: var(--c-sel);
		border: 3px solid var(--c-ring);
	}
	.timeline :global(.tl-bubble) {
		width: 12px;
		height: 12px;
		border-radius: 50%;
		background: var(--c-tile);
		border: 2px solid var(--c-mark);
		cursor: pointer;
		z-index: 1;
	}
	.timeline :global(.tl-bubble.cur) {
		background: var(--c-sel);
		border-color: var(--c-ring);
		z-index: 6;
	}
	/* Every moment's label surfaces; the layout puts it in a free slot around
	   its dot, and the label is part of the click target. Two voices: what
	   happened, and when. */
	.timeline :global(.tl-bubble-label) {
		display: flex;
		flex-direction: column;
		position: absolute;
		left: 50%;
		top: 50%;
		transform: translate(-50%, calc(-50% - 30px));
		width: max-content;
		max-width: 200px;
		text-align: center;
		background: var(--c-tile);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		padding: 4px 12px;
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 500;
		line-height: 1.25;
		color: var(--color-foreground);
		cursor: pointer;
		z-index: 2;
	}
	.timeline :global(.tl-bubble-label i) {
		font-style: normal;
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}
	.timeline :global(.tl-bubble.cur .tl-bubble-label) {
		border-color: var(--c-sel);
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
		stroke: var(--c-sel);
	}
	/* A card member keeps its shared dot and drops its label and line. */
	.timeline :global(.tl-bubble.in-card .tl-bubble-label),
	.timeline :global(.tl-bubble.in-card .tl-bubble-line) {
		display: none;
	}
	/* A stay picked from the inspector: a still ring at its place while it
	   stays picked. Nothing here breathes (design-grammar §7). */
	.timeline :global(.tl-pulse) {
		width: 20px;
		height: 20px;
		border-radius: 50%;
		background: var(--c-sel);
		border: 3px solid var(--c-ring);
		outline: 6px solid color-mix(in srgb, var(--c-sel) 22%, transparent);
		pointer-events: none;
		z-index: 3;
	}
	.timeline :global(.tl-pulse.asked) {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		color: var(--c-ring);
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 600;
		line-height: 1;
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
		cursor: pointer;
		z-index: 5;
	}
	.timeline :global(.tl-knot:hover),
	.timeline :global(.tl-knot.cur) {
		background: var(--c-sel);
	}
	/* One callout for a shared spot, its moments as rows. */
	.timeline :global(.tl-card) {
		width: max-content;
		min-width: 232px;
		max-width: 312px;
		background: var(--c-tile);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		font-family: var(--font-sans);
		color: var(--color-foreground);
		z-index: 7;
	}
	.timeline :global(.tl-card-hd) {
		display: flex;
		align-items: baseline;
		gap: 8px;
		padding: 12px 12px 8px 16px;
		border-bottom: 1px solid var(--color-border);
	}
	.timeline :global(.tl-card-hd b) {
		font-family: var(--font-serif);
		font-size: 18px;
		font-weight: 400;
		white-space: nowrap;
	}
	.timeline :global(.tl-card-hd span) {
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.timeline :global(.tl-card-x) {
		margin-left: auto;
		border: 0;
		background: none;
		color: var(--color-foreground-muted);
		font-size: 16px;
		line-height: 1;
		cursor: pointer;
		padding: 0 4px;
	}
	.timeline :global(.tl-card-x:hover) {
		color: var(--color-foreground);
	}
	.timeline :global(.tl-card-rows) {
		overflow: auto;
		padding: 4px 0;
	}
	.timeline :global(.tl-card-row) {
		display: flex;
		align-items: baseline;
		gap: 12px;
		width: 100%;
		text-align: left;
		border: 0;
		background: none;
		padding: 8px 16px;
		cursor: pointer;
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}
	.timeline :global(.tl-card-row:hover) {
		background: var(--hover-bg);
	}
	.timeline :global(.tl-card-row i) {
		font-style: normal;
		font-size: 13px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		flex: 0 0 64px;
	}
	.timeline :global(.tl-card-row span) {
		font-size: 13px;
		line-height: 1.3;
	}
	.timeline :global(.tl-card-row.cur i),
	.timeline :global(.tl-card-row.cur span) {
		color: var(--c-sel);
	}
	/* The pointer from the card to its dot. */
	.timeline :global(.tl-card-tail) {
		position: absolute;
		left: 50%;
		bottom: -6px;
		width: 12px;
		height: 12px;
		background: var(--c-tile);
		border-right: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
		transform: translateX(-50%) rotate(45deg);
	}
</style>
