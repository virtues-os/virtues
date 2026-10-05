<!--
	MobileAudioSettings — what the microphone can be told, on the Audio page.

	Three sections and a door:
	  - Recording hours: Always / Don't record during / Only record during,
	    then windows, each with the days it starts on. The plugin stores one
	    default and per-day windows that invert it (see
	    agents/plan/audio-schedule-places-plan.md); a window here is the days
	    that share one start and end, folded for editing.
	  - Never record at: the muted places, each with the radius the phone's
	    gate uses, and an "Add a place" row that opens the picker. A muted
	    place is a wiki place; this screen and the place's own page on the
	    desktop are the two doors to the same flag.
	  - Notify me if recording stops (the gap nudge).
	  - How recording works: the consent text, on its own page.

	Everything here is mute-don't-release: the mic stays armed, chunks stop
	being written, and the box is told why (a metadata-only marker).
-->
<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import Icon from "$lib/components/Icon.svelte";
	import MobilePlacePicker from "./MobilePlacePicker.svelte";
	import {
		PLACE_RADII,
		effectiveRadius,
		setPlaceMuted,
		setPlaceRadius,
		syncMutedPlaces,
		type MutedPlace,
	} from "$lib/mobile/audioPlaces";
	import {
		DAYS,
		minToTime,
		scheduleHasWindows,
		scheduleWindows,
		timeToMin,
		windowsToDays,
		type AudioStatus,
		type HoursWindow,
	} from "$lib/mobile/deviceTypes";

	interface Props {
		audio: AudioStatus;
		/** The phone's last fix, for "Use my current location". */
		fix: { lat: number; lon: number } | null;
		/** Every command resolves a full status; hand it up so the page and list agree. */
		onStatus: (s: AudioStatus) => void;
		onError: (msg: string) => void;
	}
	let { audio, fix, onStatus, onError }: Props = $props();

	const DEFAULT_WINDOW: [number, number] = [22 * 60, 7 * 60];
	const WORK_WINDOW: [number, number] = [9 * 60, 17 * 60];
	/** Day initials in `DAYS` order (Monday first). */
	const DAY_INITIALS = ["M", "T", "W", "T", "F", "S", "S"];

	async function call(cmd: string, args?: Record<string, unknown>) {
		try {
			onStatus(await invoke<AudioStatus>(`plugin:audio|${cmd}`, args));
		} catch (e) {
			onError(String(e));
		}
	}

	// ── Notify ───────────────────────────────────────────────────────────
	function toggleNotify() {
		void call("set_notify", { enabled: !audio.notify });
	}

	// ── Recording hours ─────────────────────────────────────────────────
	type Mode = "always" | "except" | "only";
	const sched = $derived(audio.schedule ?? null);
	/** A native build that predates the schedule: fall back to the quiet-hours pair. */
	const legacy = $derived(sched == null);
	const quietOn = $derived((audio.quietStart ?? -1) >= 0 && (audio.quietEnd ?? -1) >= 0);
	const windows = $derived(scheduleWindows(sched));
	const mode = $derived<Mode>(
		!sched || (!scheduleHasWindows(sched) && !sched.default_muted)
			? "always"
			: sched.default_muted
				? "only"
				: "except",
	);

	function save(defaultMuted: boolean, ws: HoursWindow[]) {
		void call("set_schedule", {
			schedule: { v: 1, default_muted: defaultMuted, days: windowsToDays(ws) },
		});
	}
	function everyDay([start, end]: [number, number]): HoursWindow {
		return { start, end, days: DAYS.map(() => true) };
	}

	function setMode(m: Mode) {
		if (m === mode) return;
		if (m === "always") return save(false, []);
		// The windows carry over; only what they mean flips.
		const ws = windows.length ? windows : [everyDay(m === "only" ? WORK_WINDOW : DEFAULT_WINDOW)];
		save(m === "only", ws);
	}
	function editWindow(i: number, patch: Partial<HoursWindow>) {
		save(
			mode === "only",
			windows.map((w, j) => (j === i ? { ...w, ...patch } : w)),
		);
	}
	function toggleDay(i: number, di: number) {
		const w = windows[i];
		// A window keeps at least one day; remove the window to drop the last.
		if (w.days[di] && w.days.filter(Boolean).length === 1) return;
		editWindow(i, { days: w.days.map((on, j) => (j === di ? !on : on)) });
	}
	function addWindow() {
		const used = DAYS.map((_, di) => windows.some((w) => w.days[di]));
		let days = used.map((u) => !u);
		if (!days.some(Boolean)) days = DAYS.map((_, di) => di < 5);
		save(mode === "only", [...windows, { start: WORK_WINDOW[0], end: WORK_WINDOW[1], days }]);
	}
	function removeWindow(i: number) {
		save(
			mode === "only",
			windows.filter((_, j) => j !== i),
		);
	}

	const hoursSentence = $derived.by(() => {
		if (mode === "always") return "Virtues records whenever the mic is on, except at the places below.";
		if (mode === "except")
			return "During these hours, Virtues doesn't record. The mic stays on, so recording picks up again when they end.";
		return "Outside these hours, Virtues doesn't record. The mic stays on, so recording picks up again when they begin.";
	});

	function setQuiet(start: number, end: number) {
		void call("set_quiet_hours", { start, end });
	}
	function toggleQuiet() {
		if (quietOn) setQuiet(-1, -1);
		else setQuiet(DEFAULT_WINDOW[0], DEFAULT_WINDOW[1]);
	}

	// ── Places ──────────────────────────────────────────────────────────
	const places = $derived(audio.places ?? []);
	const hasPlaces = $derived(audio.places != null);
	let pickerOpen = $state(false);
	let placeBusy = $state(false);
	/** The place whose radius choices are showing. */
	let openPlace = $state<string | null>(null);

	/** The box's rows → the plugin's cache, when they differ. */
	async function refreshPlaces() {
		const s = await syncMutedPlaces<AudioStatus>(audio.places);
		if (s) onStatus(s);
	}

	async function placeEdit(write: () => Promise<boolean>) {
		placeBusy = true;
		try {
			if (!(await write())) throw new Error("Your server didn't save that change. Try again.");
			await refreshPlaces();
		} catch (e) {
			onError(e instanceof Error ? e.message : String(e));
		} finally {
			placeBusy = false;
		}
	}
	function unmute(p: MutedPlace) {
		openPlace = null;
		void placeEdit(() => setPlaceMuted(p.id, false));
	}
	function setRadius(p: MutedPlace, r: number) {
		void placeEdit(() => setPlaceRadius(p.id, r));
	}

	async function pickerDone() {
		pickerOpen = false;
		await refreshPlaces();
	}

	// Copy the box's muted places into the plugin whenever this page opens.
	$effect(() => {
		if (hasPlaces) void refreshPlaces();
	});

	// ── How recording works ─────────────────────────────────────────────
	let aboutOpen = $state(false);
</script>

<div class="label">Recording hours</div>
{#if legacy}
	<div class="card">
		<button class="row" type="button" onclick={toggleQuiet}>
			<span class="r-label">Quiet hours</span>
			<span class="switch" class:on={quietOn} aria-hidden="true"></span>
		</button>
		{#if quietOn}
			<div class="row static times">
				<input
					class="time"
					type="time"
					value={minToTime(audio.quietStart ?? 0)}
					onchange={(e) => setQuiet(timeToMin(e.currentTarget.value), audio.quietEnd ?? 0)}
				/>
				<span class="r-label">to</span>
				<input
					class="time"
					type="time"
					value={minToTime(audio.quietEnd ?? 0)}
					onchange={(e) => setQuiet(audio.quietStart ?? 0, timeToMin(e.currentTarget.value))}
				/>
			</div>
		{/if}
	</div>
{:else}
	<div class="card editor">
		<div class="seg" role="radiogroup" aria-label="Recording hours">
			{#each [["always", "Always"], ["except", "Don't record during"], ["only", "Only record during"]] as [m, label] (m)}
				<button
					type="button"
					role="radio"
					aria-checked={mode === m}
					class:sel={mode === m}
					onclick={() => setMode(m as Mode)}>{label}</button
				>
			{/each}
		</div>
		{#if mode !== "always"}
			{#each windows as w, i (`${w.start}-${w.end}-${i}`)}
				<div class="win">
					<div class="times">
						<span class="t-label">From</span>
						<input
							class="time"
							type="time"
							value={minToTime(w.start)}
							onchange={(e) => editWindow(i, { start: timeToMin(e.currentTarget.value) })}
						/>
						<span class="t-label">to</span>
						<input
							class="time"
							type="time"
							value={minToTime(w.end)}
							onchange={(e) => editWindow(i, { end: timeToMin(e.currentTarget.value) })}
						/>
						{#if w.end <= w.start}<span class="next-day">next day</span>{/if}
					</div>
					<div class="starts">
						<span class="t-label">Starts on</span>
						<div class="chips">
							{#each DAY_INITIALS as d, di (di)}
								<button
									type="button"
									class="chip"
									class:on={w.days[di]}
									aria-pressed={w.days[di]}
									aria-label={DAYS[di][1]}
									onclick={() => toggleDay(i, di)}>{d}</button
								>
							{/each}
						</div>
					</div>
					{#if windows.length > 1}
						<div class="win-foot">
							<button class="linkish" type="button" onclick={() => removeWindow(i)}>Remove window</button>
						</div>
					{/if}
				</div>
			{/each}
			<div class="add">
				<button class="linkish" type="button" onclick={addWindow}>+ Add another window</button>
			</div>
		{/if}
	</div>
	<p class="fine outside">{hoursSentence}</p>
{/if}

{#if hasPlaces}
	<div class="label">Never record at</div>
	<div class="card">
		{#each places as p (p.id)}
			{@const open = openPlace === p.id}
			<button
				class="row"
				type="button"
				aria-expanded={open}
				onclick={() => (openPlace = open ? null : p.id)}
			>
				<div class="r-icon"><Icon icon="ri:map-pin-line" width={16} /></div>
				<span class="r-label r-name">{p.name || "Unnamed place"}</span>
				<span class="r-value">{effectiveRadius(p)} m</span>
				<Icon icon={open ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"} width={18} class="chev" />
			</button>
			{#if open}
				<div class="place-edit">
					<span class="t-label">Covers</span>
					{#each PLACE_RADII as r (r)}
						<button
							type="button"
							class="radius"
							class:on={effectiveRadius(p) === r}
							aria-pressed={effectiveRadius(p) === r}
							disabled={placeBusy}
							onclick={() => setRadius(p, r)}>{r} m</button
						>
					{/each}
					<button class="linkish remove" type="button" onclick={() => unmute(p)} disabled={placeBusy}>
						Remove
					</button>
				</div>
			{/if}
		{/each}
		<button class="row" type="button" onclick={() => (pickerOpen = true)} disabled={placeBusy}>
			<div class="r-icon"><Icon icon="ri:add-line" width={16} /></div>
			<span class="r-label">{places.length === 0 ? "Add a place" : "Add another place"}</span>
			<Icon icon="ri:arrow-right-s-line" width={18} class="chev" />
		</button>
	</div>
	<p class="fine outside">
		{#if audio.mutedBy === "place"}
			You're at one of these places now, so Virtues isn't recording.
		{:else if places.length === 0}
			Add a place to stop recording there. While you're inside it, Virtues doesn't record.
		{:else}
			While you're inside one of these places, Virtues doesn't record. Your record notes the gap, never the place.
		{/if}
	</p>
{/if}

<div class="card section">
	<button class="row" type="button" onclick={toggleNotify}>
		<span class="r-label">Notify me if recording stops</span>
		<span class="switch" class:on={audio.notify} aria-hidden="true"></span>
	</button>
	<button class="row" type="button" onclick={() => (aboutOpen = true)}>
		<div class="r-icon"><Icon icon="ri:information-line" width={16} /></div>
		<span class="r-label">How recording works</span>
		<Icon icon="ri:arrow-right-s-line" width={18} class="chev" />
	</button>
</div>

{#if pickerOpen}
	<MobilePlacePicker muted={places} {fix} onDone={pickerDone} onClose={() => (pickerOpen = false)} />
{/if}

{#if aboutOpen}
	<div class="about-page">
		<div class="about-head">
			<button class="back" type="button" onclick={() => (aboutOpen = false)}>
				<Icon icon="ri:arrow-left-s-line" width={22} />
				<span>Audio</span>
			</button>
		</div>
		<div class="about-body">
			<h2>How recording works</h2>
			<div class="label">What it records</div>
			<p class="about-text">
				The microphone stays on while your phone is with you. It records the sound of your day,
				including other people's voices.
			</p>
			<div class="label">Consent</div>
			<p class="about-text">
				In some places, recording a conversation needs everyone's consent. Getting it is up to you.
			</p>
			<div class="label">Where your audio goes</div>
			<p class="about-text">
				Your phone sends recordings to your server. Your server has a cloud AI model transcribe
				them, then adds the transcripts to each day's record.
			</p>
			<div class="label">When it isn't recording</div>
			<p class="about-text">
				During your recording hours' off time, or inside a place you've added, Virtues keeps
				nothing. The mic stays on, so your iPhone's orange microphone dot stays lit and recording
				picks up again on its own. Your record notes the gap, never the place.
			</p>
		</div>
	</div>
{/if}

<style>
	.label {
		font-size: 11px;
		color: var(--color-foreground-muted);
		margin: 18px 4px 8px;
	}
	.card {
		border: 1px solid var(--color-border);
		border-radius: 12px;
		overflow: hidden;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		padding: 12px 14px;
		border: 0;
		border-top: 1px solid var(--color-border);
		background: transparent;
		color: inherit;
		text-align: left;
		cursor: pointer;
	}
	.row:first-child {
		border-top: 0;
	}
	.row:disabled {
		opacity: 0.55;
	}
	.row.static {
		cursor: default;
	}
	.card.section {
		margin-top: 24px;
	}
	.editor {
		padding: 12px 14px;
	}
	.seg {
		display: flex;
		gap: 2px;
		padding: 3px;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: color-mix(in srgb, var(--wash-ink) 3%, transparent);
	}
	.seg button {
		flex: 1;
		min-height: 40px;
		padding: 6px 4px;
		border: 0;
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground-muted);
		font: inherit;
		font-size: 13px;
		line-height: 1.2;
		cursor: pointer;
	}
	.seg button:first-child {
		flex: 0.7;
	}
	.seg button.sel {
		background: var(--color-surface);
		color: var(--color-foreground);
		box-shadow: 0 0 0 1px var(--color-border);
	}
	.win {
		margin-top: 14px;
		padding-top: 14px;
		border-top: 1px solid var(--color-border);
	}
	.t-label {
		flex: none;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.next-day {
		font-size: 12px;
		color: var(--color-primary);
	}
	.starts {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 10px;
	}
	.starts .t-label {
		width: 58px;
	}
	.chips {
		display: flex;
		flex: 1;
		justify-content: space-between;
		gap: 4px;
	}
	.chip {
		width: 32px;
		height: 32px;
		padding: 0;
		border: 1px solid var(--color-border);
		border-radius: 50%;
		background: transparent;
		color: var(--color-foreground-muted);
		font: inherit;
		font-size: 13px;
		cursor: pointer;
	}
	.chip.on,
	.radius.on {
		border-color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 14%, transparent);
		color: var(--color-primary);
	}
	.win-foot {
		display: flex;
		justify-content: flex-end;
		margin-top: 8px;
	}
	.add {
		margin-top: 14px;
		padding-top: 12px;
		border-top: 1px solid var(--color-border);
	}
	.place-edit {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		padding: 0 14px 12px 52px;
	}
	.place-edit .t-label {
		margin-right: 2px;
	}
	.radius {
		padding: 4px 10px;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: transparent;
		color: var(--color-foreground-muted);
		font: inherit;
		font-size: 13px;
		cursor: pointer;
	}
	.radius:disabled {
		opacity: 0.5;
	}
	.linkish.remove {
		margin-left: auto;
	}
	.r-label {
		font-size: 15px;
		flex: 1;
		min-width: 0;
	}
	.r-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.r-value {
		font-size: 14px;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
	.row :global(.chev) {
		color: var(--color-foreground-muted);
		flex: none;
	}
	.r-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 26px;
		height: 26px;
		flex: none;
		border-radius: 7px;
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	.times {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
		justify-content: flex-start;
	}
	.times .r-label {
		flex: none;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.time {
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 4px 7px;
	}
	.fine {
		font-size: 12px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		margin: 0;
	}
	.fine.outside {
		padding: 8px 4px 0;
	}
	.about-page {
		position: fixed;
		inset: 0;
		z-index: 80;
		display: flex;
		flex-direction: column;
		background: var(--color-surface);
		color: var(--color-foreground);
		animation: slide 0.22s cubic-bezier(0.32, 0.72, 0, 1);
	}
	@keyframes slide {
		from {
			transform: translateX(28px);
			opacity: 0;
		}
	}
	.about-head {
		padding: max(10px, env(safe-area-inset-top)) 8px 4px;
	}
	.back {
		display: flex;
		align-items: center;
		gap: 2px;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 15px;
		padding: 6px 8px 6px 2px;
		cursor: pointer;
	}
	.about-body {
		flex: 1;
		overflow-y: auto;
		padding: 6px 16px max(24px, env(safe-area-inset-bottom));
	}
	.about-body h2 {
		font-size: 19px;
		font-weight: 550;
		margin: 4px 4px 4px;
	}
	.about-text {
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0 4px;
	}
	.linkish {
		border: 0;
		background: transparent;
		padding: 0;
		font: inherit;
		font-size: 13px;
		color: var(--color-primary);
		cursor: pointer;
		white-space: nowrap;
	}
	.linkish:disabled {
		opacity: 0.5;
	}
	.switch {
		flex: none;
		width: 38px;
		height: 22px;
		border-radius: 11px;
		background: var(--color-foreground-muted);
		opacity: 0.4;
		position: relative;
		transition:
			background 0.15s,
			opacity 0.15s;
	}
	.switch::after {
		content: "";
		position: absolute;
		top: 2px;
		left: 2px;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		background: #fff;
		transition: transform 0.15s;
	}
	.switch.on {
		background: var(--color-success);
		opacity: 1;
	}
	.switch.on::after {
		transform: translateX(16px);
	}
</style>
