<!--
	MobilePlacePicker — choose a place the microphone must never record at.

	A muted place IS a wiki place (the flag lives on the box's row), so this
	is a picker over the record, not a coordinate editor. Three sources, in
	the order a person reaches for them:
	  1. where they are right now ("Use my current location"),
	  2. places the record already knows — named ones by search, and the
	     unnamed clusters it has been filing visits under, which get a name
	     here and become part of the vocabulary,
	  3. anywhere else, through the box's Google Places proxy.
	Picking one mutes it on the box; the parent then copies the box's muted
	set into the native plugin.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { listPlaces, getUnnamedPlaces, type UnnamedPlace, type WikiPlaceListItem } from "$lib/wiki/api";
	import {
		createMutedPlace,
		placeDetails,
		setPlaceMuted,
		suggestPlaces,
		type MutedPlace,
		type PlaceSuggestion,
	} from "$lib/mobile/audioPlaces";

	interface Props {
		/** Places already muted — hidden from the results. */
		muted: MutedPlace[];
		/** The phone's last fix, for "Use my current location". */
		fix: { lat: number; lon: number } | null;
		/** Called after a place was muted on the box. */
		onDone: () => void;
		onClose: () => void;
	}
	let { muted, fix, onDone, onClose }: Props = $props();

	let query = $state("");
	let busy = $state(false);
	let error = $state<string | null>(null);
	let named = $state<WikiPlaceListItem[]>([]);
	let unnamed = $state<UnnamedPlace[]>([]);
	let suggestions = $state<PlaceSuggestion[]>([]);
	let loadingSources = $state(true);
	/** The one thing being named right now: an unnamed cluster, or "here". */
	let naming = $state<{ kind: "here" } | { kind: "unnamed"; place: UnnamedPlace } | null>(null);
	let newName = $state("");
	let input = $state<HTMLInputElement | null>(null);
	let suggestTimer: ReturnType<typeof setTimeout> | null = null;

	const mutedIds = $derived(new Set(muted.map((p) => p.id)));
	const q = $derived(query.trim().toLowerCase());
	const namedHits = $derived(
		named
			.filter((p) => !mutedIds.has(p.id) && (q.length === 0 || p.name.toLowerCase().includes(q)))
			.slice(0, q.length === 0 ? 8 : 12),
	);
	const unnamedHits = $derived(q.length === 0 ? unnamed.filter((p) => !mutedIds.has(p.id)).slice(0, 5) : []);

	onMount(async () => {
		input?.focus();
		const [n, u] = await Promise.all([
			listPlaces().catch(() => [] as WikiPlaceListItem[]),
			getUnnamedPlaces(20).catch(() => [] as UnnamedPlace[]),
		]);
		named = n;
		unnamed = u.filter((p) => p.latitude != null && p.longitude != null);
		loadingSources = false;
	});

	function onInput() {
		error = null;
		naming = null;
		if (suggestTimer) clearTimeout(suggestTimer);
		const text = query.trim();
		if (text.length < 3) {
			suggestions = [];
			return;
		}
		// The Google door is metered; debounce it well past typing speed.
		suggestTimer = setTimeout(async () => {
			const got = await suggestPlaces(text);
			if (query.trim() === text) suggestions = got.slice(0, 5);
		}, 500);
	}

	async function run(work: () => Promise<boolean>) {
		busy = true;
		error = null;
		try {
			if (!(await work())) throw new Error("Your server did not take that");
			onDone();
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			busy = false;
		}
	}

	function muteNamed(p: WikiPlaceListItem) {
		void run(() => setPlaceMuted(p.id, true));
	}

	function muteSuggestion(s: PlaceSuggestion) {
		void run(async () => {
			const d = await placeDetails(s.place_id);
			if (!d) throw new Error("Could not look that place up");
			return createMutedPlace(s.main_text || s.description, d.latitude, d.longitude, d.formatted_address);
		});
	}

	function distanceM(lat1: number, lon1: number, lat2: number, lon2: number): number {
		const r = 6371000;
		const dLat = ((lat2 - lat1) * Math.PI) / 180;
		const dLon = ((lon2 - lon1) * Math.PI) / 180;
		const a =
			Math.sin(dLat / 2) ** 2 +
			Math.cos((lat1 * Math.PI) / 180) * Math.cos((lat2 * Math.PI) / 180) * Math.sin(dLon / 2) ** 2;
		return 2 * r * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
	}

	function saveNamed() {
		const name = newName.trim();
		const target = naming;
		if (!name || !target) return;
		if (target.kind === "unnamed") {
			void run(() => setPlaceMuted(target.place.id, true, name));
			return;
		}
		if (!fix) {
			error = "No location fix yet";
			return;
		}
		// Prefer a cluster the record already has for this spot over minting
		// a twin: the named list carries no coordinates, so the unnamed
		// clusters (which do) are the ones checked.
		const near = unnamed.find(
			(p) => distanceM(fix.lat, fix.lon, p.latitude as number, p.longitude as number) < 100,
		);
		void run(() =>
			near ? setPlaceMuted(near.id, true, name) : createMutedPlace(name, fix.lat, fix.lon),
		);
	}

	function startNaming(target: NonNullable<typeof naming>) {
		naming = target;
		newName = "";
		error = null;
	}
</script>

<div class="picker" role="dialog" aria-label="Never record at">
	<div class="head">
		<button class="head-btn" type="button" onclick={onClose}>Cancel</button>
		<div class="head-title">Never record at</div>
		<span class="head-spacer"></span>
	</div>

	<div class="search">
		<Icon icon="ri:search-line" width={16} />
		<input
			bind:this={input}
			class="search-input"
			type="search"
			placeholder="Search your places, or anywhere"
			bind:value={query}
			oninput={onInput}
			disabled={busy}
			autocapitalize="words"
		/>
	</div>

	<div class="body">
		{#if error}
			<div class="note err">{error}</div>
		{/if}

		{#if naming}
			<div class="card">
				<div class="row static">
					<div class="r-body">
						<div class="r-title">
							{naming.kind === "here" ? "Where you are now" : `Somewhere you've been ${naming.place.ref_count} times`}
						</div>
						<div class="r-sub">Give it a name. The name is what the record will call it.</div>
					</div>
				</div>
				<div class="row static name-row">
					<input
						class="name-input"
						type="text"
						placeholder="Name this place"
						bind:value={newName}
						disabled={busy}
						autocapitalize="words"
						onkeydown={(e) => e.key === "Enter" && saveNamed()}
					/>
					<button class="act" type="button" onclick={saveNamed} disabled={busy || !newName.trim()}>
						{busy ? "…" : "Mute"}
					</button>
				</div>
			</div>
		{/if}

		{#if q.length === 0}
			<div class="card">
				<button class="row" type="button" onclick={() => startNaming({ kind: "here" })} disabled={busy || !fix}>
					<div class="r-icon"><Icon icon="ri:focus-3-line" width={17} /></div>
					<div class="r-body">
						<div class="r-title">Use my current location</div>
						<div class="r-sub">{fix ? "Name this spot and mute it" : "Waiting for a location fix"}</div>
					</div>
					<Icon icon="ri:arrow-right-s-line" width={18} class="chev" />
				</button>
			</div>
		{/if}

		{#if namedHits.length > 0}
			<div class="label">Your places</div>
			<div class="card">
				{#each namedHits as p (p.id)}
					<button class="row" type="button" onclick={() => muteNamed(p)} disabled={busy}>
						<div class="r-icon"><Icon icon="ri:map-pin-line" width={17} /></div>
						<div class="r-body">
							<div class="r-title">{p.name}</div>
							{#if p.address}<div class="r-sub">{p.address}</div>{/if}
						</div>
					</button>
				{/each}
			</div>
		{/if}

		{#if unnamedHits.length > 0}
			<div class="label">Places you've been, not yet named</div>
			<div class="card">
				{#each unnamedHits as p (p.id)}
					<button class="row" type="button" onclick={() => startNaming({ kind: "unnamed", place: p })} disabled={busy}>
						<div class="r-icon"><Icon icon="ri:map-pin-2-line" width={17} /></div>
						<div class="r-body">
							<div class="r-title">Somewhere you've been {p.ref_count} times</div>
							<div class="r-sub">
								{(p.latitude as number).toFixed(3)}, {(p.longitude as number).toFixed(3)}
							</div>
						</div>
						<Icon icon="ri:arrow-right-s-line" width={18} class="chev" />
					</button>
				{/each}
			</div>
		{/if}

		{#if suggestions.length > 0}
			<div class="label">Elsewhere</div>
			<div class="card">
				{#each suggestions as s (s.place_id)}
					<button class="row" type="button" onclick={() => muteSuggestion(s)} disabled={busy}>
						<div class="r-icon"><Icon icon="ri:global-line" width={17} /></div>
						<div class="r-body">
							<div class="r-title">{s.main_text || s.description}</div>
							{#if s.secondary_text}<div class="r-sub">{s.secondary_text}</div>{/if}
						</div>
					</button>
				{/each}
			</div>
		{/if}

		{#if !loadingSources && q.length > 0 && namedHits.length === 0 && suggestions.length === 0}
			<div class="note">
				{q.length < 3 ? "Keep typing to search beyond your own places." : "Nothing by that name."}
			</div>
		{/if}
		{#if loadingSources && q.length === 0}
			<div class="note">Loading your places…</div>
		{/if}
	</div>
</div>

<style>
	.picker {
		position: fixed;
		inset: 0;
		z-index: 80;
		display: flex;
		flex-direction: column;
		background: var(--color-surface);
		color: var(--color-foreground);
		animation: rise 0.22s cubic-bezier(0.32, 0.72, 0, 1);
	}
	@keyframes rise {
		from {
			transform: translateY(24px);
			opacity: 0;
		}
	}
	.head {
		display: flex;
		align-items: center;
		padding: max(10px, env(safe-area-inset-top)) 12px 6px;
		flex: none;
	}
	.head-btn {
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 15px;
		padding: 6px 4px;
		cursor: pointer;
		min-width: 60px;
		text-align: left;
	}
	.head-title {
		flex: 1;
		text-align: center;
		font-size: 15px;
		font-weight: 550;
	}
	.head-spacer {
		min-width: 60px;
	}
	.search {
		display: flex;
		align-items: center;
		gap: 8px;
		margin: 4px 16px 6px;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		color: var(--color-foreground-muted);
		flex: none;
	}
	.search-input {
		flex: 1;
		min-width: 0;
		border: 0;
		background: transparent;
		font: inherit;
		font-size: 15px;
		color: var(--color-foreground);
		outline: none;
	}
	.body {
		flex: 1;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		padding: 4px 16px max(24px, env(safe-area-inset-bottom));
	}
	.label {
		font-size: 11px;
		color: var(--color-foreground-muted);
		margin: 16px 4px 6px;
	}
	.card {
		border: 1px solid var(--color-border);
		border-radius: 12px;
		overflow: hidden;
		margin-top: 10px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		padding: 11px 12px;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		background: transparent;
		color: inherit;
		text-align: left;
		cursor: pointer;
	}
	.row:last-child {
		border-bottom: 0;
	}
	.row:disabled {
		opacity: 0.55;
		cursor: default;
	}
	.row.static {
		cursor: default;
	}
	.r-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		flex: none;
		border-radius: 8px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	.r-body {
		flex: 1;
		min-width: 0;
	}
	.r-title {
		font-size: 15px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.r-sub {
		font-size: 12px;
		color: var(--color-foreground-muted);
		margin-top: 1px;
	}
	.row :global(.chev) {
		color: var(--color-foreground-muted);
		flex: none;
	}
	.name-row {
		gap: 10px;
	}
	.name-input {
		flex: 1;
		min-width: 0;
		font: inherit;
		font-size: 15px;
		color: var(--color-foreground);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 7px 10px;
	}
	.act {
		flex: none;
		border: 1px solid var(--color-primary);
		color: var(--color-primary);
		background: transparent;
		border-radius: 8px;
		padding: 7px 14px;
		font-size: 13px;
		font-weight: 600;
		cursor: pointer;
	}
	.act:disabled {
		opacity: 0.5;
	}
	.note {
		font-size: 13px;
		color: var(--color-foreground-muted);
		padding: 14px 4px;
	}
	.note.err {
		color: var(--color-foreground);
	}
</style>
