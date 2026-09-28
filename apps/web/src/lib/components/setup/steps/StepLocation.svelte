<!--
	Names, the last beat — where you live (StepNames runs it after the two
	names).

	A GLOBE THAT ALREADY KNOWS. The step opens on the place the person most
	likely lives, so the usual answer is one press of "Looks right". A home
	already saved on the server wins; otherwise the guess is the computer's
	own clock (the browser's IANA zone), mapped to that zone's city. The
	guess is labeled as coming from the clock, because a zone is all the
	browser knows, and a label that claims more would be a guess wearing a
	fact's clothing.

	THE GLOBE IS DRAWN, NOT FETCHED (globe.ts, globeLand.ts): ~11,700 land
	dots bundled with the app, an orthographic projection on one canvas, and
	no tiles, no network, nothing a strict CSP has to allow. It turns in on
	arrival and settles on home; a new place is one eased turn there; the
	place's hour band is lit and the place itself breathes. It can be dragged
	and a city on it tapped. With reduced motion it stands still and jumps.

	CHANGING IT IS A SEARCH. The city line is a combobox over ~180 cities
	(cities.ts), each carrying its real IANA zone; a name the list does not
	have is kept as entered, on the current zone. The zone itself sits under
	the city and offers every zone that shares its offset. Only the zone and
	the city's name are stored — never coordinates.

	SAVE CONTRACT (unchanged): updateProfile({ home_timezone, home_city }),
	then setup.refresh(), then onnext().
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { browser } from "$app/environment";
	import Icon from "$lib/components/Icon.svelte";
	import { updateProfile } from "$lib/api/client";
	import { setup } from "../setup.svelte";
	import { CITIES, cityForZone, type City } from "../cities";
	import { Globe } from "../globe";
	import StepFrame from "../StepFrame.svelte";

	let { eyebrow, onnext, onback }: { eyebrow?: string; onnext: () => void; onback?: () => void } = $props();

	const yourName = $derived(setup.profile?.preferred_name?.trim() || "");

	// ── zones ─────────────────────────────────────────────────────────────

	/** Minutes east of UTC for a zone, today. */
	function offsetOf(zone: string, at = new Date()): number | null {
		try {
			const part = new Intl.DateTimeFormat("en-US", { timeZone: zone, timeZoneName: "shortOffset" })
				.formatToParts(at)
				.find((p) => p.type === "timeZoneName")?.value;
			if (!part) return null;
			if (part === "GMT") return 0;
			const m = part.match(/GMT([+-])(\d{1,2})(?::(\d{2}))?/);
			if (!m) return null;
			const mins = Number(m[2]) * 60 + Number(m[3] ?? 0);
			return m[1] === "-" ? -mins : mins;
		} catch {
			return null;
		}
	}

	const ZONES: { zone: string; offset: number }[] = (() => {
		if (!browser || !("supportedValuesOf" in Intl)) return [];
		const all = (Intl as unknown as { supportedValuesOf(k: string): string[] }).supportedValuesOf("timeZone");
		return all
			.filter((z) => z.includes("/") && !z.startsWith("Etc/"))
			.map((zone) => ({ zone, offset: offsetOf(zone) }))
			.filter((z): z is { zone: string; offset: number } => z.offset !== null);
	})();

	const leafOf = (zone: string) => zone.split("/").pop()!.replace(/_/g, " ");
	/** A zone is a place when it names one: "America/Chicago", not "UTC" or "Etc/GMT+5". */
	const isPlaceZone = (zone: string) => zone.includes("/") && !zone.startsWith("Etc/");

	/** "UTC−5", "UTC+5:30". A true minus sign, the way a clock face sets it. */
	function formatOffset(mins: number): string {
		if (mins === 0) return "UTC";
		const sign = mins < 0 ? "−" : "+";
		const a = Math.abs(mins);
		const h = Math.floor(a / 60);
		const m = a % 60;
		return `UTC${sign}${h}${m ? `:${String(m).padStart(2, "0")}` : ""}`;
	}

	/** The zone's offset without daylight saving: the smaller of January's and July's. */
	function standardOffset(zone: string): number {
		const y = new Date().getFullYear();
		const jan = offsetOf(zone, new Date(Date.UTC(y, 0, 1)));
		const jul = offsetOf(zone, new Date(Date.UTC(y, 6, 1)));
		return Math.min(jan ?? jul ?? 0, jul ?? jan ?? 0);
	}

	/** "Central Time", "Japan Time"; the zone's leaf when the engine has no name. */
	function zoneName(zone: string): string {
		try {
			const n = new Intl.DateTimeFormat("en-US", { timeZone: zone, timeZoneName: "longGeneric" })
				.formatToParts(new Date())
				.find((p) => p.type === "timeZoneName")?.value;
			if (n && !n.startsWith("GMT")) return n;
		} catch {
			/* fall through */
		}
		return leafOf(zone);
	}

	/**
	 * Somewhere to stand for a zone the city list does not carry
	 * (America/Indiana/Indianapolis): a listed city in the same region on the
	 * same offset, else any on the same offset. It only aims the globe; the
	 * name shown is still the zone's own.
	 */
	function standInFor(zone: string): City | null {
		const off = offsetOf(zone);
		if (off === null) return null;
		const region = zone.split("/")[0] + "/";
		return (
			CITIES.find((c) => c.zone.startsWith(region) && offsetOf(c.zone) === off) ??
			CITIES.find((c) => offsetOf(c.zone) === off) ??
			null
		);
	}

	// ── where to start: saved beats guessed ───────────────────────────────

	const browserZone = browser ? Intl.DateTimeFormat().resolvedOptions().timeZone : "UTC";
	// A datacenter box writes UTC before anyone has said anything; that is not an answer.
	const savedZone = setup.profile?.home_timezone && setup.profile.home_timezone !== "UTC" ? setup.profile.home_timezone : null;
	const savedCity = setup.profile?.home_city?.trim() || null;
	const initialZone = savedZone ?? browserZone;
	const listed = savedCity
		? (CITIES.find((c) => c.name === savedCity && c.zone === initialZone) ?? CITIES.find((c) => c.name === savedCity) ?? null)
		: null;
	const zoneCity = cityForZone(initialZone);
	const anchor = listed ?? zoneCity ?? standInFor(initialZone);
	const initialCity = savedCity ?? zoneCity?.name ?? (isPlaceZone(initialZone) ? leafOf(initialZone) : "");
	/** Nothing saved, and the zone is the computer's own: the step is offering a guess. */
	const guessed = !savedCity && initialZone === browserZone && !!initialCity;

	let zone = $state(initialZone);
	let city = $state(initialCity);
	/** Where the globe points. A typed name the list lacks keeps the last place. */
	let place = $state<{ lat: number; lng: number } | null>(anchor ? { lat: anchor.lat, lng: anchor.lng } : null);
	let saving = $state(false);
	let error = $state<string | null>(null);

	const offset = $derived(offsetOf(zone) ?? 0);
	const unchanged = $derived(city.trim() === initialCity && zone === initialZone);
	const inBand = $derived(
		ZONES.filter((z) => z.offset === offset).sort((a, b) => leafOf(a.zone).localeCompare(leafOf(b.zone))),
	);

	// The clock there, so the choice can be checked against a watch.
	let now = $state(new Date());
	const clock = $derived.by(() => {
		try {
			return new Intl.DateTimeFormat(undefined, { timeZone: zone, hour: "numeric", minute: "2-digit" }).format(now);
		} catch {
			return "";
		}
	});

	// ── the globe ─────────────────────────────────────────────────────────

	let canvas = $state<HTMLCanvasElement | null>(null);
	let globe: Globe | null = null;

	onMount(() => {
		globe = new Globe(canvas!, { onpick: choose });
		aim();
		const tick = setInterval(() => (now = new Date()), 15_000);
		return () => {
			clearInterval(tick);
			globe?.destroy();
			globe = null;
		};
	});

	const band = $derived(standardOffset(zone));

	function aim() {
		globe?.setTarget(place ? { ...place, label: city.trim(), offset: band } : null);
	}

	$effect(() => {
		// Re-aim whenever the place, its name or its hour changes.
		void place;
		void city;
		void band;
		aim();
	});

	function choose(c: City) {
		place = { lat: c.lat, lng: c.lng };
		city = c.name;
		zone = c.zone;
		query = c.name;
		open = false;
		error = null;
	}

	/** Arrow keys on the globe walk to the nearest city in that direction. */
	function globeKey(e: KeyboardEvent) {
		const dirs: Record<string, [number, number]> = {
			ArrowLeft: [-1, 0],
			ArrowRight: [1, 0],
			ArrowUp: [0, 1],
			ArrowDown: [0, -1],
		};
		const d = dirs[e.key];
		if (!d) return;
		e.preventDefault();
		const from = place ?? { lat: 20, lng: 0 };
		let best: City | null = null;
		let bestScore = Infinity;
		for (const c of CITIES) {
			const dx = ((((c.lng - from.lng + 180) % 360) + 360) % 360 - 180) * Math.cos((from.lat * Math.PI) / 180);
			const dy = c.lat - from.lat;
			const along = dx * d[0] + dy * d[1];
			if (along <= 0.5) continue;
			const score = along + Math.abs(dx * d[1] - dy * d[0]) * 2.5;
			if (score < bestScore) {
				bestScore = score;
				best = c;
			}
		}
		if (best) choose(best);
	}

	// ── the search ────────────────────────────────────────────────────────

	let query = $state(initialCity);
	let open = $state(false);
	let active = $state(0);
	let input = $state<HTMLInputElement | null>(null);

	const fold = (s: string) =>
		s
			.normalize("NFD")
			.replace(/[̀-ͯ]/g, "")
			.toLowerCase()
			.trim();

	const matches = $derived.by(() => {
		const q = fold(query);
		if (!q || q === fold(city)) return [];
		const scored: { c: City; s: number }[] = [];
		for (const c of CITIES) {
			const n = fold(c.name);
			const s = n.startsWith(q)
				? 0
				: n.split(/[\s.-]+/).some((w) => w.startsWith(q))
					? 1
					: fold(c.country).startsWith(q)
						? 2
						: n.includes(q)
							? 3
							: -1;
			if (s >= 0) scored.push({ c, s });
		}
		// A match in the middle of a name ("Lis" in Minneapolis) only counts
		// when nothing starts with what was entered: a name, a word of one, or
		// a country.
		const starts = scored.some((x) => x.s < 3);
		return scored
			.filter((x) => !starts || x.s < 3)
			.sort((a, b) => a.s - b.s || a.c.name.localeCompare(b.c.name))
			.slice(0, 6)
			.map((x) => x.c);
	});

	function onInput() {
		open = true;
		active = 0;
	}

	/** Keep what was entered: a listed city if it is one, else the name on the current zone. */
	function commit() {
		const q = query.trim();
		if (!q) {
			query = city;
			open = false;
			return;
		}
		const exact = CITIES.find((c) => fold(c.name) === fold(q));
		if (exact) choose(exact);
		else {
			city = q;
			open = false;
		}
	}

	function searchKey(e: KeyboardEvent) {
		if (e.key === "ArrowDown" && matches.length) {
			e.preventDefault();
			open = true;
			active = (active + 1) % matches.length;
		} else if (e.key === "ArrowUp" && matches.length) {
			e.preventDefault();
			active = (active - 1 + matches.length) % matches.length;
		} else if (e.key === "Enter") {
			e.preventDefault();
			if (open && matches[active]) choose(matches[active]);
			else if (fold(query) !== fold(city)) commit();
			else save();
		} else if (e.key === "Escape") {
			if (open || query !== city) {
				e.preventDefault();
				query = city;
				open = false;
			}
		}
	}

	const showList = $derived(open && matches.length > 0);

	// ── save ──────────────────────────────────────────────────────────────

	async function save() {
		if (fold(query) !== fold(city)) commit();
		const c = city.trim();
		if (!c || saving) return;
		saving = true;
		error = null;
		try {
			await updateProfile({ home_timezone: zone, home_city: c });
			await setup.refresh();
			onnext();
		} catch {
			error = "Your server couldn't save its location. Try again.";
			saving = false;
		}
	}
</script>

<StepFrame
	{eyebrow}
	centered
	title={yourName ? `Where's home, ${yourName}?` : "Where's home?"}
	subtitle="Your server starts each new day at midnight where you live."
>
	<div class="stage">
		<canvas
			bind:this={canvas}
			class="globe"
			role="slider"
			tabindex="0"
			aria-label="Globe. Use the arrow keys to move between cities."
			aria-valuetext="{city || 'No city'}, {formatOffset(offset)}"
			aria-valuenow={offset}
			onkeydown={globeKey}
		></canvas>
	</div>

	<div class="place">
		<div class="search">
			<input
				bind:this={input}
				bind:value={query}
				class="setup-field city"
				role="combobox"
				aria-label="City"
				aria-autocomplete="list"
				aria-expanded={showList}
				aria-controls="home-cities"
				aria-activedescendant={showList ? `home-city-${active}` : undefined}
				placeholder="Enter a city"
				autocomplete="off"
				spellcheck="false"
				maxlength="80"
				oninput={onInput}
				onfocus={() => input?.select()}
				onblur={commit}
				onkeydown={searchKey}
			/>
			<Icon icon="ri:search-line" width="16" class="search-icon" />
			{#if showList}
				<ul id="home-cities" class="list" role="listbox" aria-label="Cities">
					{#each matches as c, i (c.name + c.zone)}
						<li
							id="home-city-{i}"
							role="option"
							aria-selected={i === active}
							class:active={i === active}
							onpointerdown={(e) => {
								e.preventDefault();
								choose(c);
							}}
							onpointerenter={() => (active = i)}
						>
							<span class="opt-name">{c.name}</span>
							<span class="opt-country">{c.country}</span>
							<span class="opt-offset">{formatOffset(offsetOf(c.zone) ?? 0)}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</div>

		<div class="meta">
			<label class="zone">
				<span class="zone-text">{zoneName(zone)} · {formatOffset(offset)}</span>
				<Icon icon="ri:arrow-down-s-line" width="14" />
				<select
					aria-label="Time zone"
					value={zone}
					onchange={(e) => (zone = (e.currentTarget as HTMLSelectElement).value)}
				>
					{#each inBand as z (z.zone)}
						<option value={z.zone}>{leafOf(z.zone)} · {formatOffset(z.offset)}</option>
					{/each}
					{#if !inBand.some((z) => z.zone === zone)}
						<option value={zone}>{leafOf(zone)}</option>
					{/if}
				</select>
			</label>
			{#if clock}
				<span class="sep" aria-hidden="true">·</span>
				<time class="clock">{clock}</time>
			{/if}
		</div>
		{#if guessed}
			<!-- Kept in the layout after a change, so the way forward does not jump. -->
			<p class="hint" class:gone={!unchanged} aria-hidden={!unchanged}>From your computer's clock</p>
		{/if}
	</div>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#snippet actions()}
		<button type="button" class="setup-go" disabled={!city.trim() || saving} onclick={save}>
			{saving ? "Saving…" : unchanged && initialCity ? "Looks right" : `Set home to ${city.trim() || "this city"}`}
			<Icon icon="ri:arrow-right-line" width="16" />
		</button>
		{#if onback}
			<button type="button" class="setup-past" disabled={saving} onclick={onback}>Back</button>
		{/if}
	{/snippet}
</StepFrame>

<style>
	.stage {
		display: flex;
		justify-content: center;
		margin-top: -8px;
	}
	.globe {
		display: block;
		/* As large as the window allows with the title, the city, its time
		   and the way forward all still on screen: the rest of the step takes
		   about 540px, so a 1280×800 window gets a 260px globe and the button
		   above the fold. 360 at most, never under 200. */
		width: min(100%, 360px, max(200px, 100svh - 540px));
		aspect-ratio: 1;
		cursor: grab;
		/* A vertical swipe still scrolls the page on a phone; a sideways one turns the globe. */
		touch-action: pan-y;
		user-select: none;
		-webkit-user-select: none;
		-webkit-tap-highlight-color: transparent;
		border-radius: 50%;
	}
	.globe:global([data-dragging]) {
		cursor: grabbing;
	}
	.globe:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 4px;
	}

	.place {
		display: flex;
		flex-direction: column;
		align-items: center;
		margin-top: 20px;
		animation: setup-rise var(--m-slow) var(--m-ease) both;
		animation-delay: 480ms;
	}

	.search {
		position: relative;
		width: min(100%, 22em);
	}
	.city {
		width: 100%;
		padding-left: 24px;
		padding-right: 24px;
	}
	.search :global(.search-icon) {
		position: absolute;
		left: 0;
		top: 8px;
		color: var(--color-foreground-subtle, var(--color-foreground-muted));
		pointer-events: none;
		opacity: 0;
		transition: opacity var(--m-quick) ease;
	}
	.search:focus-within :global(.search-icon) {
		opacity: 1;
	}

	.list {
		position: absolute;
		z-index: 5;
		top: calc(100% + 8px);
		left: 0;
		right: 0;
		margin: 0;
		padding: 4px;
		list-style: none;
		text-align: left;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		animation: list-in var(--m-quick) var(--m-ease) both;
	}
	@keyframes list-in {
		from {
			opacity: 0;
			transform: translateY(-4px);
		}
	}
	.list li {
		display: flex;
		align-items: baseline;
		gap: 8px;
		padding: 8px 12px;
		border-radius: 6px;
		cursor: pointer;
	}
	.list li.active {
		background: color-mix(in srgb, var(--color-primary) 9%, transparent);
	}
	.opt-name {
		font-size: 15px;
		color: var(--color-foreground);
	}
	.opt-country {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.opt-offset {
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}

	.meta {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 12px;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.zone {
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: 4px;
		cursor: pointer;
		transition: color var(--m-quick) ease;
	}
	.zone:hover,
	.zone:focus-within {
		color: var(--color-foreground);
	}
	.zone select {
		position: absolute;
		inset: 0;
		width: 100%;
		opacity: 0;
		cursor: pointer;
		font: inherit;
	}
	.zone:has(select:focus-visible) {
		outline: 2px solid var(--color-primary);
		outline-offset: 4px;
		border-radius: 6px;
	}
	.sep {
		color: var(--color-foreground-subtle, var(--color-foreground-muted));
	}
	.clock {
		font-variant-numeric: tabular-nums;
	}
	.hint {
		transition: opacity var(--m-base) var(--m-ease);
		margin: 8px 0 0;
		font-size: 12px;
		color: var(--color-foreground-subtle, var(--color-foreground-muted));
	}

	.hint.gone {
		opacity: 0;
	}

	.error {
		margin: 16px 0 0;
		text-align: center;
		font-size: 13px;
		color: var(--color-error);
	}

	@media (max-width: 560px) {
		.globe {
			width: min(100%, 320px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.place,
		.list {
			animation: none;
		}
	}
</style>
