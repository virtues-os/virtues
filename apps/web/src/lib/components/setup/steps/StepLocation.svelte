<!--
	Names, the last beat — where you live (StepNames runs it after the two
	names).

	A whole-world chart that does not zoom, one oversized pin, and the hour
	band under the pin lit. Every choice says the same thing: precision is not
	wanted here. The server needs two facts — the time zone its days turn
	over in, and the city it calls home — and the pin is the way to both.

	THE MAP IS DRAWN, NOT FETCHED. It is a graticule of the 24 hour meridians
	and ~180 cities as points of light (cities.ts): no tiles, no coastline
	data, nothing asked of any third party. It shows exactly the detail the
	step wants — cities, never streets or shops — and the continents read
	from the cities alone. (The app's tile maps go through a proxy whose
	upstream began refusing keyless requests on 2026-09-23, which this step
	would have inherited.)

	THE PIN SNAPS TO A CITY. Dragging moves it freely and the band follows
	the pointer; letting go settles it on the nearest city, whose name fills
	the City field and whose real IANA zone becomes the time zone. The person
	sees the city they will be saved as and can retype it; the zone list
	offers every zone that shares the offset. Only the zone and the city's
	name are stored — never the pin's coordinates, which are a gesture at a
	map, not a place.

	The browser's zone was already written before the letter (a datacenter
	box reads UTC), so this step refines rather than supplies — which is why
	it never blocks the steps after it.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { browser } from "$app/environment";
	import Icon from "$lib/components/Icon.svelte";
	import { updateProfile } from "$lib/api/client";
	import { setup } from "../setup.svelte";
	import { CITIES, nearestCity, cityForZone, type City } from "../cities";
	import StepFrame from "../StepFrame.svelte";

	let { eyebrow, onnext, onback }: { eyebrow?: string; onnext: () => void; onback?: () => void } = $props();

	// The conversation the two names started carries on: centered, and asked
	// in the name the person just gave.
	const yourName = $derived(setup.profile?.preferred_name?.trim() || "");

	// ── the chart's projection ────────────────────────────────────────────
	// Equirectangular, cropped to where people live: 75°N to 58°S.
	const W = 1000;
	const NORTH = 75;
	const SOUTH = -58;
	const H = ((NORTH - SOUTH) / 360) * W;
	const x = (lng: number) => ((lng + 180) / 360) * W;
	const y = (lat: number) => ((NORTH - lat) / 360) * W;
	const lngAt = (px: number) => (px / W) * 360 - 180;
	const latAt = (py: number) => NORTH - (py / W) * 360;
	const BAND = (15 / 360) * W;

	// ── zones ─────────────────────────────────────────────────────────────

	/** Minutes east of UTC for a zone, today. */
	function offsetOf(zone: string): number | null {
		try {
			const part = new Intl.DateTimeFormat("en-US", { timeZone: zone, timeZoneName: "shortOffset" })
				.formatToParts(new Date())
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

	/** "UTC−5", "UTC+5:30". A true minus sign, the way a clock face sets it. */
	function formatOffset(mins: number): string {
		if (mins === 0) return "UTC";
		const sign = mins < 0 ? "−" : "+";
		const a = Math.abs(mins);
		const h = Math.floor(a / 60);
		const m = a % 60;
		return `UTC${sign}${h}${m ? `:${String(m).padStart(2, "0")}` : ""}`;
	}

	// ── state ─────────────────────────────────────────────────────────────

	const browserZone = browser ? Intl.DateTimeFormat().resolvedOptions().timeZone : "UTC";
	const savedZone = setup.profile?.home_timezone;
	const initialZone = savedZone && savedZone !== "UTC" ? savedZone : browserZone;
	const savedCity = setup.profile?.home_city ?? null;
	const initialCity: City | null =
		(savedCity && CITIES.find((c) => c.name === savedCity)) || cityForZone(initialZone);

	let zone = $state(initialZone);
	let city = $state(savedCity ?? initialCity?.name ?? leafOf(initialZone));
	let pinned = $state<City | null>(initialCity);
	/** Where the pin is drawn, in chart units. */
	let pin = $state(
		initialCity
			? { x: x(initialCity.lng), y: y(initialCity.lat) }
			: { x: x(((offsetOf(initialZone) ?? 0) / 60) * 15), y: y(30) },
	);
	let dragging = $state(false);
	let hover = $state<City | null>(null);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let arrived = $state(false);

	const offset = $derived(offsetOf(zone) ?? 0);
	/** The whole-hour band: under the pointer while dragging, the zone's otherwise. */
	const bandHour = $derived(dragging ? Math.round(lngAt(pin.x) / 15) : Math.round(offset / 60));
	const bandLabel = $derived(dragging ? formatOffset(bandHour * 60) : formatOffset(offset));
	const inBand = $derived(
		ZONES.filter((z) => z.offset === offset).sort((a, b) => leafOf(a.zone).localeCompare(leafOf(b.zone))),
	);

	onMount(() => {
		// Let the chart draw in, then light the cities west to east.
		requestAnimationFrame(() => requestAnimationFrame(() => (arrived = true)));
	});

	// ── the pin ───────────────────────────────────────────────────────────

	let svg = $state<SVGSVGElement | null>(null);

	function toChart(e: PointerEvent): { x: number; y: number } {
		const r = svg!.getBoundingClientRect();
		return {
			x: Math.max(0, Math.min(W, ((e.clientX - r.left) / r.width) * W)),
			y: Math.max(0, Math.min(H, ((e.clientY - r.top) / r.height) * H)),
		};
	}

	function settleOn(c: City) {
		pinned = c;
		pin = { x: x(c.lng), y: y(c.lat) };
		city = c.name;
		zone = c.zone;
	}

	function down(e: PointerEvent) {
		if (!svg) return;
		svg.setPointerCapture(e.pointerId);
		dragging = true;
		hover = null;
		pin = toChart(e);
	}
	function move(e: PointerEvent) {
		if (!svg) return;
		const p = toChart(e);
		if (dragging) {
			pin = p;
			return;
		}
		// Hovering names the city under the pointer, within a small reach.
		const c = nearestCity(latAt(p.y), lngAt(p.x));
		hover = Math.hypot(x(c.lng) - p.x, y(c.lat) - p.y) < 14 ? c : null;
	}
	function up(e: PointerEvent) {
		if (!dragging) return;
		dragging = false;
		const p = toChart(e);
		settleOn(nearestCity(latAt(p.y), lngAt(p.x)));
	}

	/** Arrow keys walk the pin to the nearest city in that direction. */
	function key(e: KeyboardEvent) {
		const dirs: Record<string, [number, number]> = {
			ArrowLeft: [-1, 0],
			ArrowRight: [1, 0],
			ArrowUp: [0, -1],
			ArrowDown: [0, 1],
		};
		const d = dirs[e.key];
		if (!d) return;
		e.preventDefault();
		let best: City | null = null;
		let bestScore = Infinity;
		for (const c of CITIES) {
			const dx = x(c.lng) - pin.x;
			const dy = y(c.lat) - pin.y;
			const along = dx * d[0] + dy * d[1];
			if (along <= 2) continue;
			const score = along + Math.abs(dx * d[1] - dy * d[0]) * 2.5;
			if (score < bestScore) {
				bestScore = score;
				best = c;
			}
		}
		if (best) settleOn(best);
	}

	function chooseZone(z: string) {
		zone = z;
		const c = cityForZone(z);
		if (c) {
			pinned = c;
			pin = { x: x(c.lng), y: y(c.lat) };
		}
	}

	async function save() {
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

	const meridians = Array.from({ length: 25 }, (_, i) => -180 + i * 15);
	const parallels = [60, 30, 0, -30];
</script>

<StepFrame
	{eyebrow}
	centered
	title={yourName ? `Where's home, ${yourName}?` : "Where's home?"}
	subtitle="Drag the pin to your city, so your server's days end at your midnight."
>
	<div class="chart" class:arrived class:dragging>
		<svg
			bind:this={svg}
			viewBox="0 0 {W} {H}"
			role="slider"
			tabindex="0"
			aria-label="Your server's location. Use the arrow keys to move between cities."
			aria-valuetext="{pinned?.name ?? city}, {formatOffset(offset)}"
			aria-valuenow={offset}
			onpointerdown={down}
			onpointermove={move}
			onpointerup={up}
			onpointercancel={up}
			onpointerleave={() => (hover = null)}
			onkeydown={key}
		>
			<defs>
				<linearGradient id="loc-band" x1="0" y1="0" x2="0" y2="1">
					<stop offset="0" stop-color="var(--color-primary)" stop-opacity="0.02" />
					<stop offset="0.5" stop-color="var(--color-primary)" stop-opacity="0.12" />
					<stop offset="1" stop-color="var(--color-primary)" stop-opacity="0.02" />
				</linearGradient>
				<radialGradient id="loc-glow">
					<stop offset="0" stop-color="var(--color-primary)" stop-opacity="0.32" />
					<stop offset="1" stop-color="var(--color-primary)" stop-opacity="0" />
				</radialGradient>
			</defs>

			<!-- the hour band, sliding between hours rather than jumping -->
			<g class="band" style:transform="translateX({x(bandHour * 15 - 7.5)}px)">
				<rect x="0" y="0" width={BAND} height={H} fill="url(#loc-band)" />
				<line x1="0" y1="0" x2="0" y2={H} />
				<line x1={BAND} y1="0" x2={BAND} y2={H} />
				<text class="band-label" x={BAND / 2} y="20">{bandLabel}</text>
			</g>

			<!-- the graticule: the 24 hour meridians and four parallels -->
			<g class="grid">
				{#each meridians as m}
					<line x1={x(m)} y1="0" x2={x(m)} y2={H} />
				{/each}
				{#each parallels as p}
					<line class:equator={p === 0} x1="0" y1={y(p)} x2={W} y2={y(p)} />
				{/each}
			</g>

			<!-- the cities, lit west to east as the chart arrives -->
			<g class="cities">
				{#each CITIES as c (c.name + c.zone)}
					<circle
						class="city"
						class:near={Math.abs(c.lng - bandHour * 15) <= 7.5}
						cx={x(c.lng)}
						cy={y(c.lat)}
						r="2.4"
						style:transition-delay="{(x(c.lng) / W) * 1100}ms"
					/>
				{/each}
			</g>

			{#if hover && hover !== pinned}
				<text class="hover-label" x={x(hover.lng)} y={y(hover.lat) - 9}>{hover.name}</text>
			{/if}

			<!-- the pin -->
			<g class="pin" style:transform="translate({pin.x}px, {pin.y}px)">
				<circle class="glow" r="36" fill="url(#loc-glow)" />
				<circle class="ring" r="15" />
				<circle class="dot" r="7" />
				{#if pinned && !dragging}
					{#key pinned.name}
						<text class="pin-label" y="-24">{pinned.name}</text>
					{/key}
				{/if}
			</g>
		</svg>
	</div>

	<div class="fields">
		<label class="field">
			<span class="label">City</span>
			<input class="input" bind:value={city} autocomplete="address-level2" maxlength="80" />
		</label>
		<label class="field">
			<span class="label">Time zone</span>
			<select class="input" value={zone} onchange={(e) => chooseZone((e.currentTarget as HTMLSelectElement).value)}>
				{#each inBand as z (z.zone)}
					<option value={z.zone}>{leafOf(z.zone)} · {formatOffset(z.offset)}</option>
				{/each}
				{#if !inBand.some((z) => z.zone === zone)}
					<option value={zone}>{leafOf(zone)}</option>
				{/if}
			</select>
		</label>
	</div>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	{#snippet actions()}
		<button type="button" class="setup-go" disabled={!city.trim() || saving} onclick={save}>
			{saving ? "Saving…" : `Set home to ${city.trim() || "this city"}`}
			<Icon icon="ri:arrow-right-line" width="16" />
		</button>
		{#if onback}
			<button type="button" class="setup-past" disabled={saving} onclick={onback}>Back</button>
		{/if}
	{/snippet}
</StepFrame>

<style>
	.chart {
		position: relative;
		width: 100%;
		border-radius: 16px;
		overflow: hidden;
		background:
			radial-gradient(120% 90% at 50% 40%, color-mix(in srgb, var(--color-primary) 3%, transparent), transparent 70%),
			color-mix(in srgb, var(--color-foreground) 2.5%, var(--color-surface));
		box-shadow: inset 0 0 0 1px var(--color-border);
	}
	svg {
		display: block;
		width: 100%;
		height: auto;
		cursor: grab;
		touch-action: none;
		user-select: none;
		-webkit-user-select: none;
	}
	.dragging svg {
		cursor: grabbing;
	}
	svg:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		border-radius: 16px;
	}

	.grid line {
		stroke: var(--color-foreground);
		stroke-opacity: 0.06;
		stroke-width: 1;
		vector-effect: non-scaling-stroke;
	}
	.grid line.equator {
		stroke-opacity: 0.13;
		stroke-dasharray: 3 4;
	}

	.band {
		transition: transform 420ms cubic-bezier(0.2, 0.7, 0.2, 1);
	}
	.dragging .band {
		transition-duration: 180ms;
	}
	.band line {
		stroke: var(--color-primary);
		stroke-opacity: 0.35;
		stroke-width: 1;
		vector-effect: non-scaling-stroke;
	}
	.band-label {
		text-anchor: middle;
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		fill: var(--color-primary);
	}

	.city {
		fill: var(--color-foreground);
		opacity: 0;
		transition:
			opacity 700ms ease,
			fill 300ms ease;
	}
	.arrived .city {
		opacity: 0.26;
	}
	.arrived .city.near {
		opacity: 0.62;
		transition-delay: 0ms !important;
	}

	.hover-label,
	.pin-label {
		text-anchor: middle;
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		pointer-events: none;
		paint-order: stroke;
		stroke: var(--color-surface);
		stroke-width: 4px;
		stroke-linejoin: round;
	}
	.hover-label {
		font-size: 13px;
		fill: var(--color-foreground-muted);
	}
	.pin-label {
		font-size: 18px;
		fill: var(--color-foreground);
		animation: label-in 460ms ease both;
	}
	@keyframes label-in {
		from {
			opacity: 0;
		}
	}

	/* The pin: oversized on purpose, and it settles onto its city. */
	.pin {
		transition: transform 560ms cubic-bezier(0.2, 0.8, 0.2, 1);
		pointer-events: none;
	}
	.dragging .pin {
		transition: none;
	}
	.pin .dot {
		fill: var(--color-primary);
		stroke: var(--color-surface);
		stroke-width: 3;
	}
	.pin .ring {
		fill: color-mix(in srgb, var(--color-primary) 14%, transparent);
		transform-box: fill-box;
		transform-origin: center;
		animation: breathe 2.8s ease-in-out infinite;
	}
	.dragging .pin .ring {
		animation: none;
		transform: scale(1.35);
	}
	@keyframes breathe {
		0%,
		100% {
			transform: scale(0.7);
			opacity: 0.9;
		}
		50% {
			transform: scale(1.15);
			opacity: 0.4;
		}
	}

	.fields {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1rem;
		margin-top: 1.5rem;
	}
	@media (max-width: 560px) {
		.fields {
			grid-template-columns: 1fr;
		}
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
	}
	.label {
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.input {
		width: 100%;
		font: inherit;
		font-size: 15px;
		padding: 0.6rem 0.8rem;
		border-radius: 10px;
		border: 1px solid var(--color-border);
		background: var(--color-surface);
		color: var(--color-foreground);
	}
	.input:focus {
		outline: none;
		border-color: color-mix(in srgb, var(--color-primary) 60%, transparent);
	}

	.error {
		margin: 1rem 0 0;
		font-size: 13px;
		color: var(--color-error);
	}

	@media (prefers-reduced-motion: reduce) {
		.band,
		.pin,
		.city {
			transition: none;
		}
		.pin .ring,
		.pin-label {
			animation: none;
		}
	}
</style>
