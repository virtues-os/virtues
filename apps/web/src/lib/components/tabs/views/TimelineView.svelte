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
	import { onMount, onDestroy, tick, untrack } from 'svelte';
	import { browser } from '$app/environment';
	import { atlasStyle } from '$lib/map/atlas';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import type { GeoJSONSource, LngLat, Map as MlMap, Marker, StyleSpecification } from 'maplibre-gl';
	import { fetchDayBounds, fetchDayWindow, fetchDerived, fetchLanes, fetchVoice, lastMeasured, localDay, quietDayVerdict, stepDay } from '$lib/timeline/day';
	import { cleanTrack, dropSpikes, flagHoles, splitTrack, toFixes, type Fix, type Line } from '$lib/timeline/track';
	import { buildRail, type RailPick, type RailSection } from '$lib/timeline/rail';
	import { anchorAt, positionAt, trackMetres } from '$lib/timeline/anchor';
	import { Bubbles, type Area, type MapMoment } from '$lib/timeline/bubbles';
	import TimelineRail from '$lib/components/timeline/TimelineRail.svelte';
	import TimelineScrubber from '$lib/components/timeline/TimelineScrubber.svelte';
	import TimelineMonth from '$lib/components/timeline/TimelineMonth.svelte';
	import { laneData, NO_LANES, type Lanes } from '$lib/timeline/lanes';
	import { buildFolds, clampView, DAY, HOUR, MIN, midnightIn, tierOf, tierView, unwarp, warp, zoneOffset, type Tier } from '$lib/timeline/scale';
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
	let root = $state<HTMLElement | null>(null);
	/** The pane's own width (split view, a small window), which decides where
	 *  the top cards go. */
	let paneW = $state(0);
	/** The date card, its month and the scope switcher float on the map as
	 *  separate cards; their measures place everything else. */
	let dateWrap = $state<HTMLElement | null>(null);
	let cardW = $state(0);
	let cardH = $state(0);
	let segW = $state(0);
	let segH = $state(0);
	let dayW = $state(0);
	let barHt = $state(0);
	let scrubH = $state(0);
	let monthOpen = $state(false);
	/** The scope. Life isn't built yet, so Day is the only one to pick; the
	 *  scope bar's grow and shrink run once a second scope is live. */
	let scope = $state<'day' | 'life'>('day');
	/** The one material every card reads: solid, or the frosted variant, kept
	 *  switchable (dev builds only) until the owner picks. */
	const dev = import.meta.env.DEV;
	let material = $state<'solid' | 'frosted'>(readMaterial());
	function readMaterial(): 'solid' | 'frosted' {
		try {
			return localStorage.getItem('timeline.material') === 'frosted' ? 'frosted' : 'solid';
		} catch {
			// No storage (a private window): the default material.
			return 'solid';
		}
	}
	function setMaterial(m: 'solid' | 'frosted') {
		material = m;
		try {
			localStorage.setItem('timeline.material', m);
		} catch {
			// No storage: the choice lasts until the page reloads.
		}
	}
	/** The scope switcher sits at the top centre unless it would meet the
	 *  date card; then it goes under the card. */
	const stacked = $derived(paneW > 0 && paneW / 2 - segW / 2 < 16 + cardW + 16);
	const navTop = $derived(stacked ? 16 + cardH + 8 : 16);
	/** Where the scope bar ends: the Reset pill and a quiet day's note go under it. */
	const navBottom = $derived(navTop + segH + 6 + barHt);
	/** The bottom of the top cards, for the map's framing and clear area. */
	const topChrome = $derived(Math.max(16 + cardH, navBottom));
	/** The rail runs full height, unless the date card would run into it. */
	const railLow = $derived(paneW > 0 && 16 + cardW + 16 > paneW - Math.min(384, paneW * 0.42) - 16);
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
	/** The user has moved the playhead in this day; until then the rail opens
	 *  nothing by itself (main.js:1559). */
	let armed = $state(false);
	let dayStart = $state(0);
	let dayEnd = $state(0);
	let pin: Marker | null = null;
	let bubbles: Bubbles | null = null;
	/** Which stretch of path is lit, so it is written only when it changes. */
	let litKey = '';
	/** What the user last pointed at, for the rail's scroll (main.js:1559). */
	let railFocus = $state<'row' | 'sec'>('row');

	/** The scrubber's view, the span on screen (main.js:10). It stays put while
	 *  the playhead moves, and scrolls only to keep the playhead in it. */
	let viewStart = $state(0);
	let viewEnd = $state(0);
	let playing = $state(false);
	/** What the lanes draw: the day, then the week around it once the view
	 *  leaves the day. */
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

	const today = getLocalDateSlug();
	let date = $state(today);
	let ready = $state(false);
	let basemap = $state(true);
	let status = $state<'loading' | 'shown' | 'empty' | 'error'>('loading');
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
			// The prototype's gestures (dayback/src/main.js:988-993, 1870-1875): a
			// pinch is the only zoom, two fingers pan, nothing rotates, and no
			// double-click, drag-box or keyboard zoom (the keys belong to the
			// playhead).
			m.doubleClickZoom.disable();
			m.boxZoom.disable();
			m.keyboard.disable();
			m.dragRotate.disable();
			m.touchZoomRotate.disableRotation();
			container.addEventListener('wheel', onWheel, { passive: false, capture: true });
			m.on('move', syncAway);
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

	// The day loads on its own, and the map draws it once the map is up: the
	// bar, the rail and a day's note never wait on the tiles (the prototype
	// draws its rail with no map at all).
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

	async function loadDay(slug: string) {
		const mine = ++asked;
		status = 'loading';
		note = null;
		let bounds, window, derived, voice, lw;
		try {
			// The day in the zone it woke up in, then every fix around it, the
			// server's stays, drives and moments over it, what the mic heard, and
			// the scrubber's steps and calendar.
			bounds = await fetchDayBounds(slug);
			[window, derived, voice, lw] = await Promise.all([
				fetchDayWindow(bounds.startMs, bounds.endMs),
				fetchDerived(bounds).catch((e) => {
					console.warn('[Timeline] derived fetch failed', e);
					return null;
				}),
				fetchVoice(bounds).catch((e) => {
					console.warn('[Timeline] voice fetch failed', e);
					return [];
				}),
				fetchLanes(bounds).catch((e) => {
					console.warn('[Timeline] lanes fetch failed', e);
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
		sections = derived?.is_built ? buildRail(derived, startMs, endMs, voice) : [];
		// Spikes go first, then holes are judged across the whole window, then the
		// day is cut out of it.
		const all = flagHoles(dropSpikes(toFixes(window.points)));
		const fixes = all.filter((f) => f.t >= startMs && f.t < endMs);
		dayFixes = fixes;
		dayTrack = cleanTrack(fixes);
		dayStart = startMs;
		dayEnd = endMs;
		lanes = laneData(derived, voice, lw, startMs, endMs);
		midnights = Array.from({ length: 9 }, (_, i) => midnightIn(stepDay(slug, i - 4), bounds.zone));
		// A stepped-to day opens whole, at midnight, untouched (main.js:1387);
		// a nudge lands at its time of day, in its tier.
		cancelAnim();
		playT = startMs;
		armed = false;
		setView(startMs, endMs);
		const nudged = pendingNudge;
		pendingNudge = null;
		if (nudged) {
			setTier(nudged.tier, true);
			park(startMs + nudged.tod);
		}
		railFocus = 'row';
		litKey = '';
		lastFix = all.filter((f) => f.t < startMs).at(-1) ?? toFixes(window.before ? [window.before] : [])[0] ?? null;
		status = fixes.length ? 'shown' : 'empty';
		drawn++;
		if (fixes.length) return;
		// No fix all day. The prototype's rule (`honestWhere` + `gapWhy`): hold the
		// last position, say when it was measured and what the phone did meanwhile.
		const measured = lastFix ? lastMeasured(lastFix.t, false, zone) : null;
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

	/** The loaded day on the map: its track, then the frame, the bubbles and
	 *  the playhead; a day with no fix holds the last position. */
	async function drawDay() {
		const m = map;
		if (!m) return;
		const mine = asked;
		const { runs, bridges } = splitTrack(dayTrack);
		(m.getSource('track-run') as GeoJSONSource).setData(lines(runs));
		(m.getSource('track-bridge') as GeoJSONSource).setData(lines(bridges));
		// The rail draws first, so the framing can leave room for it.
		await tick();
		if (mine !== asked) return;
		if (dayFixes.length) {
			frameDay(m, 0);
			bubbles?.build(mapMoments());
			syncPlayhead();
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

	const sentence = (s: string) => s.charAt(0).toUpperCase() + s.slice(1);

	/** The prototype's framing padding (`v4FitPad`, dayback/src/main.js:1190):
	 *  the measured top cards, rail and scrubber plus a buffer of max(34 px,
	 *  6 % of the smaller side) on every edge, each capped so fitBounds can
	 *  still move. */
	function fitPad(m: MlMap) {
		const c = m.getContainer();
		const cw = c.clientWidth || 900;
		const ch = c.clientHeight || 600;
		const buf = Math.max(34, Math.round(Math.min(cw, ch) * 0.06));
		const railW = rail?.width() ?? 0;
		return {
			top: Math.min(topChrome + buf, ch * 0.34),
			bottom: Math.min(scrubBottom() + buf, ch * 0.45),
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
		// This camera is home: Reset view appears once you leave it.
		if (duration) m.once('moveend', recordHome);
		else recordHome();
	}

	/** Two fingers pan the map the way they move; a pinch (the browser sends
	 *  it as a wheel with ctrl) is left to MapLibre, which zooms about the
	 *  cursor; over a place card the card's list scrolls (main.js:988-992). */
	function onWheel(e: WheelEvent) {
		if (e.ctrlKey || e.metaKey) return;
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
		const m = map;
		if (!m) return;
		bubbles?.fold();
		if (dayFixes.length) frameDay(m, 650);
		else if (home) m.easeTo({ center: home.c, zoom: home.z, duration: 650 });
	}
	/** The keys (main.js:792-802, 1205): Esc is Reset view; ← → nudge the
	 *  playhead, with Shift they jump between conversations; Space plays.
	 *  Only while the Timeline is on screen, and never while typing. */
	function onKey(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			if (monthOpen) {
				e.preventDefault();
				monthOpen = false;
			} else if (away && !e.defaultPrevented) resetView();
			return;
		}
		if (!root?.getClientRects().length || e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey) return;
		const el = e.target as HTMLElement | null;
		const tag = el?.tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || el?.isContentEditable) return;
		if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
			e.preventDefault();
			const dir = e.key === 'ArrowRight' ? 1 : -1;
			if (e.shiftKey) jumpConvo(dir);
			else nudge(dir);
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
		armed = true;
		playT = Math.max(dayStart, Math.min(dayEnd - 1000, t));
		ensureVisible();
		syncPlayhead();
	}

	/** The latest the view may reach, two days past the record (main.js:95). */
	const viewHi = () => localDay(today).endMs + 2 * DAY;
	function setView(a: number, b: number) {
		[viewStart, viewEnd] = clampView(a, b, Number.NEGATIVE_INFINITY, viewHi());
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
		const [a, b] = tierView(t, { s: dayStart, e: dayEnd }, playT);
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
		railFocus = 'row';
		park(t);
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
		// Play is a touch too.
		armed = true;
		railFocus = 'row';
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
		if (next >= dayEnd) {
			playing = false;
			loopRaf = 0;
			loopTs = null;
			return;
		}
		playT = next;
		ensureVisible();
		syncPlayhead();
		loopRaf = requestAnimationFrame(loop);
	}

	/** ← → move the playhead one unit of the lit tier (main.js:1383-1386): a
	 *  minute or an hour inside the day; a day or a week crosses to that day,
	 *  keeping the time of day and the tier, never past today. */
	function nudge(dir: 1 | -1) {
		const t = tier;
		if (t === 'min' || t === 'hour') {
			railFocus = 'row';
			park(playT + dir * (t === 'min' ? MIN : HOUR));
			return;
		}
		let next = stepDay(date, dir * (t === 'week' ? 7 : 1));
		if (next > today) next = today;
		if (next === date) return;
		pendingNudge = { tier: t, tod: playT - dayStart };
		date = next;
	}
	/** Shift+← → glide the view to the middle of the next or previous
	 *  conversation (`jumpConvo`, main.js:791-794). */
	function jumpConvo(dir: 1 | -1) {
		const target = dir > 0 ? lanes.talk.find((m) => m > playT + 1000) : lanes.talk.findLast((m) => m < playT - 1000);
		if (target === undefined) return;
		const span = viewEnd - viewStart;
		glideView(target - span / 2, target + span / 2);
	}

	// The week's lanes load the first time the view leaves the day: the Week
	// tier, or a pinch out past midnight.
	let wideAsked = 0;
	$effect(() => {
		if (!lanes.b || (viewStart >= lanes.a && viewEnd <= lanes.b)) return;
		untrack(() => void loadWide());
	});
	async function loadWide() {
		const mine = asked;
		if (wideAsked === mine) return;
		wideAsked = mine;
		const b = { startMs: dayStart - 3 * DAY, endMs: dayStart + 4 * DAY, zone };
		const [derived, voice, lw] = await Promise.all([
			fetchDerived(b).catch((e) => {
				console.warn('[Timeline] week derived fetch failed', e);
				return null;
			}),
			fetchVoice(b).catch((e) => {
				console.warn('[Timeline] week voice fetch failed', e);
				return [];
			}),
			fetchLanes(b).catch((e) => {
				console.warn('[Timeline] week lanes fetch failed', e);
				return null;
			}),
		]);
		if (mine !== asked) return;
		lanes = laneData(derived, voice, lw, b.startMs, b.endMs);
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

	/** The clear map (`v4ClearArea`, main.js:1042-1044): not under the top
	 *  cards, the rail or the scrubber, 8 px in from every edge. */
	function clearArea(m: MlMap): Area {
		const c = m.getContainer();
		const railW = rail?.width() ?? 0;
		return { l: 8, t: topChrome + 8, r: c.clientWidth - (railW ? railW + 20 : 0) - 8, b: c.clientHeight - scrubBottom() - 8 };
	}

	/** What the scrubber takes from the bottom: its card and 26 px under it,
	 *  220 before it has drawn (main.js:1044, 1192). */
	const scrubBottom = () => (scrubH ? scrubH + 26 : 220);

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
	class:stacked
	class:with-rail={sections.length > 0}
	data-material={material}
	bind:this={root}
	bind:clientWidth={paneW}
	style="{colourVars}; --nav-top: {navTop}px; --nav-bottom: {navBottom}px; --scrub-h: {scrubH}px; --rail-top: {railLow ? topChrome + 12 : 16}px"
>
	<div class="timeline-map" bind:this={container}></div>

	<!-- No header band: the map runs edge to edge and every control floats on
	     it as its own card. The date card, top left: the date, left-aligned
	     like Calendar's month title, with ‹ Today › and the month beside it. -->
	<div class="date" bind:this={dateWrap}>
		<div class="date-card tile" bind:offsetWidth={cardW} bind:offsetHeight={cardH}>
			<!-- The day's standout line goes at the start of this row, in small
			     caps, once our significance exists (TFP 2.3.5.14); until then the
			     row holds only the day's controls, never a guessed line. -->
			<div class="date-row">
				<button class="date-btn" aria-label="Previous day" title="Previous day" onclick={() => (date = stepDay(date, -1))}>‹</button>
				<button class="date-btn" disabled={date === today} onclick={() => (date = today)}>Today</button>
				<button class="date-btn" aria-label="Next day" title="Next day" disabled={date >= today} onclick={() => (date = stepDay(date, 1))}>›</button>
				<button class="date-btn" aria-label="Show the month" title="Show the month" aria-expanded={monthOpen} onclick={() => (monthOpen = !monthOpen)}>
					<svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true"><rect x="3.5" y="5" width="17" height="15.5" rx="3" /><path d="M3.5 10h17M8 3v4M16 3v4" /></svg>
				</button>
			</div>
			<p class="date-title">{title}</p>
		</div>
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

	<!-- The scope switcher, top centre, where Calendar keeps Day Week Month
	     Year: its labels are scopes, and "Dayline" stays the feature's name.
	     Only built scopes are live; Year is left out and Life shows greyed.
	     Map | Detail is Day's own scope bar, hanging off the Day segment. -->
	<nav class="scope" aria-label="Scope" data-scope={scope}>
		<div class="scope-seg tile" role="tablist" bind:offsetWidth={segW} bind:offsetHeight={segH}>
			<button class="seg on" role="tab" aria-selected="true" title="Dayline - a single day" bind:offsetWidth={dayW}>Day</button>
			<button class="seg" role="tab" aria-selected="false" aria-disabled="true" data-tip="Coming soon">Life</button>
		</div>
		<div class="scope-bar tile" role="tablist" aria-label="Day view" bind:offsetHeight={barHt} style="transform-origin: {4 + dayW / 2}px 0">
			<button class="seg on" role="tab" aria-selected="true" title="Map - the day on a map"><i aria-hidden="true">🌐</i>Map</button>
			<button class="seg" role="tab" aria-selected="false" aria-disabled="true" data-tip="Coming soon"><i aria-hidden="true">🔍</i>Detail</button>
		</div>
	</nav>

	<TimelineRail bind:this={rail} {sections}
		{zone}
		{playT}
		{armed}
		focus={railFocus}
		onpick={reveal}
		onseek={(t) => {
			railFocus = 'row';
			park(t);
		}}
	/>
	{#if dayStart}
		<TimelineScrubber
			bind:cardHeight={scrubH}
			{viewStart}
			{viewEnd}
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
			hasFinance={lanes.hasFinance}
			onseek={scrubSeek}
			onzoom={zoom}
			ontier={(t) => setTier(t)}
			onplay={togglePlay}
		/>
	{/if}
	{#if railError && status !== 'error' && status !== 'loading'}
		<p class="rail-error tile">Your server couldn't load the day's stays. Reload the page to try again.</p>
	{/if}

	{#if status === 'empty' && note}
		<div class="note tile" class:below={away}>
			<p class="note-title">{note.title}</p>
			{#each note.lines as line (line)}
				<p class="note-line">{line}</p>
			{/each}
		</div>
	{:else if status === 'error'}
		<div class="note tile" class:below={away}><p class="note-title">Your server couldn't load {label}. Reload the page to try again.</p></div>
	{/if}
	{#if away}
		<!-- Apple's re-centre pattern: it exists only once the camera has left the
		     day's frame (dayback/index.html:822-826, 1180). -->
		<button class="reset" type="button" title="Back to the whole day (Esc)" onclick={resetView}>
			<i aria-hidden="true">⤢</i>Reset view
		</button>
	{/if}
	{#if !basemap}
		<p class="no-basemap">Your server has no map tiles yet</p>
	{/if}
	{#if dev}
		<!-- Dev builds only, until the material is picked: solid or frosted. -->
		<div class="material tile" role="group" aria-label="Material (dev only)">
			<span>Material</span>
			<button class:on={material === 'solid'} onclick={() => setMaterial('solid')}>Solid</button>
			<button class:on={material === 'frosted'} onclick={() => setMaterial('frosted')}>Frosted</button>
		</div>
	{/if}
</div>

<style>
	/* One material for every card - the date card, the scope switcher and
	   its scope bar, the rail and the scrubber - so they always match. Solid
	   is the default; frosted is the variant, kept switchable until picked. */
	.timeline {
		position: absolute;
		inset: 0;
		--tile-bg: var(--c-tile);
		--tile-border: 1px solid color-mix(in srgb, var(--color-foreground) 7%, transparent);
		--tile-radius: 12px;
		--tile-blur: none;
	}
	.timeline[data-material='frosted'] {
		--tile-bg: color-mix(in srgb, var(--c-tile) 66%, transparent);
		--tile-border: 1px solid color-mix(in srgb, var(--color-foreground) 6%, transparent);
		--tile-blur: blur(30px) saturate(180%);
	}
	/* The map's markers stack at z 1 to 7 (bubbles, chips, the place card);
	   everything over the map sits at 10 and up, as the prototype's tiles sit
	   above its markers. */
	.timeline-map {
		position: absolute;
		inset: 0;
	}
	/* A card over the map, in the prototype's look: a ground, a faint ink
	   border and one soft shadow (dayback/index.html:599-601), all from the
	   material above. */
	.tile {
		background: var(--tile-bg);
		border: var(--tile-border);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--tile-shadow);
		-webkit-backdrop-filter: var(--tile-blur);
		backdrop-filter: var(--tile-blur);
	}
	/* The date card, top left. */
	.date {
		position: absolute;
		top: 16px;
		left: 16px;
		z-index: 12;
	}
	.date-card {
		/* design-ok: the mockup's date card inset (owner's call, 2026-09-30) */
		padding: 11px 12px 14px 18px;
		border-radius: var(--tile-radius);
	}
	.date-row {
		display: flex;
		justify-content: flex-end;
		/* design-ok: the mockup's control spacing (owner's call, 2026-09-30) */
		gap: 4px;
	}
	.date-btn {
		height: 26px;
		min-width: 26px;
		/* design-ok: the mockup's small round control (owner's call, 2026-09-30) */
		padding: 0 9px;
		border: 0;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 600;
		line-height: 1;
		cursor: pointer;
		display: inline-flex;
		align-items: center;
		justify-content: center;
	}
	.date-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
	}
	.date-btn:disabled {
		opacity: 0.35;
		cursor: default;
	}
	.date-btn[aria-expanded='true'] {
		background: var(--color-foreground);
		color: var(--color-background);
	}
	.date-btn svg {
		fill: none;
		stroke: currentColor;
		stroke-width: 2;
		stroke-linecap: round;
	}
	/* The serif page title, as Virtues' Home dateline: regular, never bold. */
	.date-title {
		/* design-ok: the date sits under its controls, as in the mockup (owner's call, 2026-09-30) */
		margin: 9px 0 0;
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 36px;
		letter-spacing: -0.02em;
		line-height: 1;
		white-space: nowrap;
		color: var(--color-foreground);
	}
	/* The scope switcher, top centre; it never moves. Its scope bar hangs
	   off the Day segment and grows out of it, or shrinks back into it when
	   another scope is picked. */
	.scope {
		position: absolute;
		top: var(--nav-top);
		left: 50%;
		transform: translateX(-50%);
		z-index: 11;
	}
	.stacked .scope {
		left: 16px;
		transform: none;
	}
	.scope-seg,
	.scope-bar {
		display: inline-flex;
		padding: 4px;
		border-radius: var(--tile-radius);
	}
	.scope-bar {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		white-space: nowrap;
		/* A generous clip leaves the shadow whole at rest. */
		clip-path: inset(-40px round var(--tile-radius));
		transition:
			transform 0.38s cubic-bezier(0.2, 0.9, 0.25, 1.08),
			clip-path 0.38s cubic-bezier(0.2, 0.9, 0.25, 1.08),
			opacity 0.2s ease;
	}
	.scope:not([data-scope='day']) .scope-bar {
		opacity: 0;
		pointer-events: none;
		transform: translateY(-12px) scale(0.6, 0.5);
		clip-path: inset(0 55% 0 0 round var(--tile-radius));
	}
	@media (prefers-reduced-motion: reduce) {
		.scope-bar {
			transition: opacity 0.2s ease;
		}
		.scope:not([data-scope='day']) .scope-bar {
			transform: none;
		}
	}
	/* The scope bar is Day's child: smaller type, a tighter segment. */
	.scope-bar .seg {
		font-size: 12px;
		/* design-ok: the Dayback prototype's lens segment (owner's call, 2026-09-30) */
		padding: 7px 12px;
	}
	.scope-bar .seg i {
		font-style: normal;
		font-size: 11px;
		line-height: 1;
		/* design-ok: the Dayback prototype's lens icon gap (owner's call, 2026-09-30) */
		margin-right: 5px;
	}
	.seg {
		border: 0;
		background: none;
		/* design-ok: the Dayback prototype's toggle segment (owner's call, 2026-09-30) */
		padding: 10px 15px;
		/* Concentric with the card's corner at a 4 px inset. */
		border-radius: calc(var(--tile-radius) - 4px);
		font-family: var(--font-sans);
		font-weight: 600;
		font-size: 14px;
		line-height: 1;
		letter-spacing: -0.005em;
		white-space: nowrap;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.seg.on {
		color: var(--color-foreground);
		background: var(--color-background);
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--toggle-shadow);
	}
	/* A scale not built yet (index.html:974). It stays hoverable (so no
	   `disabled`, which would swallow the hover) and does nothing on click. */
	.seg[aria-disabled='true'] {
		position: relative;
		color: color-mix(in srgb, var(--color-foreground-subtle) 55%, transparent);
		cursor: default;
	}
	.seg[aria-disabled='true'] i {
		opacity: 0.55;
	}
	/* "Coming soon", a tenth of a second after the pointer arrives - the
	   browser's own title tooltip waits about a second. */
	.seg[data-tip]::after {
		content: attr(data-tip);
		position: absolute;
		top: calc(100% + 8px);
		left: 50%;
		transform: translateX(-50%);
		padding: 4px 8px;
		border-radius: 6px;
		background: var(--color-foreground);
		color: var(--color-background);
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 500;
		white-space: nowrap;
		pointer-events: none;
		opacity: 0;
		transition: opacity 120ms ease;
		z-index: 5;
	}
	.seg[data-tip]:hover::after,
	.seg[data-tip]:focus-visible::after {
		opacity: 1;
		transition-delay: 100ms;
	}
	@media (prefers-reduced-motion: reduce) {
		.seg[data-tip]::after {
			transition: none;
		}
	}
	/* The Reset view pill: solid ink, paper text, so it reads as a control and
	   not a tile (index.html:822-826). The prototype set it at the top centre;
	   here the top centre is the scope switcher, so it sits under its scope
	   bar. */
	.reset {
		position: absolute;
		top: calc(var(--nav-bottom) + 12px);
		left: 50%;
		transform: translateX(-50%);
		z-index: 11;
		display: inline-flex;
		align-items: center;
		/* design-ok: the Dayback prototype's pill (owner's call, 2026-09-30) */
		gap: 7px;
		/* design-ok: the Dayback prototype's pill (owner's call, 2026-09-30) */
		padding: 7px 14px 7px 12px;
		border: 0;
		border-radius: 999px;
		background: var(--color-foreground);
		color: var(--color-background);
		font-family: var(--font-sans);
		/* design-ok: the Dayback prototype's pill (owner's call, 2026-09-30) */
		font-size: 12.5px;
		font-weight: 600;
		cursor: pointer;
		/* design-ok: the Timeline follows the Dayback prototype's look (owner's call, 2026-09-30) */
		box-shadow: var(--tile-shadow);
	}
	.reset:hover {
		background: color-mix(in srgb, var(--color-foreground) 88%, var(--color-background));
	}
	.reset i {
		font-style: normal;
		font-size: 14px;
		line-height: 1;
	}
	/* Under the scope bar, not over the middle: the map holds the last known
	   position at its centre, and the note must not hide it. */
	.note {
		position: absolute;
		z-index: 10;
		top: calc(var(--nav-bottom) + 12px);
		left: 50%;
		transform: translateX(-50%);
		padding: 8px 14px;
		border-radius: var(--tile-radius);
		text-align: center;
	}
	.note.below {
		top: calc(var(--nav-bottom) + 60px);
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
		z-index: 10;
		top: 16px;
		right: 16px;
		max-width: min(384px, 42%);
		margin: 0;
		padding: 12px 16px;
		border-radius: var(--tile-radius);
		font-size: 13px;
		color: var(--color-foreground);
	}
	/* Dev builds only: the material switch, just above the scrubber. */
	.material {
		position: absolute;
		left: 16px;
		bottom: calc(var(--scrub-h) + 26px);
		z-index: 10;
		display: inline-flex;
		align-items: center;
		/* design-ok: a dev-only switch, removed before the pull request */
		gap: 4px;
		/* design-ok: a dev-only switch, removed before the pull request */
		padding: 4px 4px 4px 10px;
		border-radius: var(--tile-radius);
		font-family: var(--font-sans);
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.material button {
		border: 0;
		background: none;
		/* design-ok: a dev-only switch, removed before the pull request */
		padding: 5px 9px;
		border-radius: calc(var(--tile-radius) - 4px);
		font: inherit;
		color: inherit;
		cursor: pointer;
	}
	.material button.on {
		background: var(--color-foreground);
		color: var(--color-background);
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
