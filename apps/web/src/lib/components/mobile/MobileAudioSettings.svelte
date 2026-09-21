<!--
	MobileAudioSettings — what the microphone can be told, on the Audio page.

	Three settings and a footer:
	  - Notify me if recording stops (the gap nudge).
	  - Recording hours: a value row ("Always", "Except 10:00 PM – 7:00 AM",
	    "Weekdays 9–5", …) that opens the editor beneath it. One default and
	    windows that invert it, so quiet hours and record-at-work-only are the
	    same store — see agents/plan/audio-schedule-places-plan.md.
	  - Never record at: the muted places, and an "Add a place…" row that
	    opens the picker. A muted place is a wiki place; this screen and the
	    place's own page on the desktop are the two doors to the same flag.
	  - The consent text, as a footer, once the mic is on.

	Everything here is mute-don't-release: the mic stays armed, chunks stop
	being written, and the box is told why (a metadata-only marker).
-->
<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import Icon from "$lib/components/Icon.svelte";
	import MobilePlacePicker from "./MobilePlacePicker.svelte";
	import { setPlaceMuted, syncMutedPlaces, type MutedPlace } from "$lib/mobile/audioPlaces";
	import {
		DAYS,
		describeSchedule,
		fmtClock,
		minToTime,
		scheduleHasWindows,
		timeToMin,
		uniformWindow,
		type AudioStatus,
		type MuteSchedule,
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
	const sched = $derived(audio.schedule ?? null);
	/** A native build that predates the schedule: fall back to the quiet-hours pair. */
	const legacy = $derived(sched == null);
	const quietOn = $derived((audio.quietStart ?? -1) >= 0 && (audio.quietEnd ?? -1) >= 0);
	const hoursValue = $derived.by(() => {
		if (!legacy) return describeSchedule(sched);
		return quietOn
			? `Except ${fmtClock(audio.quietStart ?? 0)} – ${fmtClock(audio.quietEnd ?? 0)}`
			: "Always";
	});
	let hoursOpen = $state(false);
	const uniform = $derived(uniformWindow(sched));
	const windowsExist = $derived(scheduleHasWindows(sched));
	let byDay = $state(false);
	const showByDay = $derived(byDay || (windowsExist && uniform == null));

	function setSchedule(doc: MuteSchedule) {
		void call("set_schedule", { schedule: { v: 1, default_muted: doc.default_muted, days: doc.days } });
	}
	function uniformDoc(w: [number, number] | null, defaultMuted: boolean): MuteSchedule {
		const days: Record<string, [number, number][]> = {};
		for (const [k] of DAYS) days[k] = w ? [w] : [];
		return { default_muted: defaultMuted, days };
	}
	/** "Always" ⇄ a window. The editor opens on the first window. */
	function toggleHours() {
		if (legacy) {
			if (quietOn) void call("set_quiet_hours", { start: -1, end: -1 });
			else void call("set_quiet_hours", { start: DEFAULT_WINDOW[0], end: DEFAULT_WINDOW[1] });
			return;
		}
		if (!sched) return;
		if (windowsExist || sched.default_muted) {
			setSchedule(uniformDoc(null, false));
			hoursOpen = false;
		} else {
			setSchedule(uniformDoc(DEFAULT_WINDOW, false));
			hoursOpen = true;
		}
	}
	const hoursOn = $derived(legacy ? quietOn : windowsExist || !!sched?.default_muted);
	function setDefaultMuted(muted: boolean) {
		if (sched) setSchedule({ ...sched, default_muted: muted });
	}
	function setUniform(start: number, end: number) {
		if (sched) setSchedule(uniformDoc([start, end], sched.default_muted));
	}
	function setDay(day: string, w: [number, number] | null) {
		if (sched) setSchedule({ ...sched, days: { ...sched.days, [day]: w ? [w] : [] } });
	}
	function dayWindow(day: string): [number, number] | null {
		return sched?.days[day]?.[0] ?? null;
	}
	function toggleByDay() {
		if (showByDay) {
			byDay = false;
			const w = DAYS.map(([k]) => dayWindow(k)).find((x) => x != null) ?? null;
			if (sched) setSchedule(uniformDoc(w, sched.default_muted));
		} else {
			byDay = true;
		}
	}
	function setQuiet(start: number, end: number) {
		void call("set_quiet_hours", { start, end });
	}

	// ── Places ──────────────────────────────────────────────────────────
	const places = $derived(audio.places ?? []);
	const hasPlaces = $derived(audio.places != null);
	let pickerOpen = $state(false);
	let placeBusy = $state(false);

	/** The box's rows → the plugin's cache, when they differ. */
	async function refreshPlaces() {
		const s = await syncMutedPlaces<AudioStatus>(audio.places);
		if (s) onStatus(s);
	}

	async function unmute(p: MutedPlace) {
		placeBusy = true;
		try {
			if (!(await setPlaceMuted(p.id, false))) throw new Error("Your server did not take that");
			await refreshPlaces();
		} catch (e) {
			onError(e instanceof Error ? e.message : String(e));
		} finally {
			placeBusy = false;
		}
	}

	async function pickerDone() {
		pickerOpen = false;
		await refreshPlaces();
	}

	// Copy the box's muted places into the plugin whenever this page opens.
	$effect(() => {
		if (hasPlaces) void refreshPlaces();
	});
</script>

<div class="label">Settings</div>
<div class="card">
	<button class="row" type="button" onclick={toggleNotify}>
		<span class="r-label">Notify me if recording stops</span>
		<span class="switch" class:on={audio.notify} aria-hidden="true"></span>
	</button>

	<button class="row" type="button" onclick={() => (hoursOpen = !hoursOpen)}>
		<span class="r-label">Recording hours</span>
		<span class="r-value">{hoursValue}</span>
		<Icon icon={hoursOpen ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line"} width={18} class="chev" />
	</button>
	{#if hoursOpen}
		<div class="editor">
			<button class="row sub" type="button" onclick={toggleHours}>
				<span class="r-label">{legacy ? "Quiet hours" : "Limit recording"}</span>
				<span class="switch" class:on={hoursOn} aria-hidden="true"></span>
			</button>
			{#if legacy}
				{#if quietOn}
					<div class="row sub times">
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
			{:else if sched && hoursOn}
				<div class="row sub">
					<span class="r-label">Outside these hours</span>
					<select
						class="time"
						value={sched.default_muted ? "muted" : "record"}
						onchange={(e) => setDefaultMuted(e.currentTarget.value === "muted")}
					>
						<option value="record">record</option>
						<option value="muted">don't record</option>
					</select>
				</div>
				<button class="row sub" type="button" onclick={toggleByDay}>
					<span class="r-label">Different each day</span>
					<span class="switch" class:on={showByDay} aria-hidden="true"></span>
				</button>
				{#if !showByDay}
					<div class="row sub times">
						<input
							class="time"
							type="time"
							value={minToTime(uniform?.[0] ?? DEFAULT_WINDOW[0])}
							onchange={(e) => setUniform(timeToMin(e.currentTarget.value), uniform?.[1] ?? DEFAULT_WINDOW[1])}
						/>
						<span class="r-label">to</span>
						<input
							class="time"
							type="time"
							value={minToTime(uniform?.[1] ?? DEFAULT_WINDOW[1])}
							onchange={(e) => setUniform(uniform?.[0] ?? DEFAULT_WINDOW[0], timeToMin(e.currentTarget.value))}
						/>
					</div>
				{:else}
					{#each DAYS as [key, name] (key)}
						{@const w = dayWindow(key)}
						<div class="row sub times day">
							<button class="day-toggle" type="button" onclick={() => setDay(key, w ? null : DEFAULT_WINDOW)}>
								<span class="switch small" class:on={w != null} aria-hidden="true"></span>
								<span class="r-label day-name">{name.slice(0, 3)}</span>
							</button>
							{#if w}
								<input
									class="time"
									type="time"
									value={minToTime(w[0])}
									onchange={(e) => setDay(key, [timeToMin(e.currentTarget.value), w[1]])}
								/>
								<span class="r-label">to</span>
								<input
									class="time"
									type="time"
									value={minToTime(w[1])}
									onchange={(e) => setDay(key, [w[0], timeToMin(e.currentTarget.value)])}
								/>
							{/if}
						</div>
					{/each}
				{/if}
			{/if}
			<p class="fine">
				{#if !hoursOn}
					The microphone records whenever it is on.
				{:else if sched?.default_muted}
					Nothing is kept outside these hours. The mic stays on so it can resume without you.
				{:else}
					Nothing is kept during these hours. The mic stays on so it can resume without you.
				{/if}
			</p>
		</div>
	{/if}
</div>

{#if hasPlaces}
	<div class="label">Never record at</div>
	<div class="card">
		{#each places as p (p.id)}
			<div class="row static">
				<div class="r-icon"><Icon icon="ri:map-pin-line" width={16} /></div>
				<span class="r-label r-name">{p.name || "Unnamed place"}</span>
				<span class="r-value">{Math.round(p.radiusM)} m</span>
				<button class="linkish" type="button" onclick={() => unmute(p)} disabled={placeBusy}>Remove</button>
			</div>
		{/each}
		<button class="row" type="button" onclick={() => (pickerOpen = true)} disabled={placeBusy}>
			<div class="r-icon"><Icon icon="ri:add-line" width={16} /></div>
			<span class="r-label">{places.length === 0 ? "Add a place" : "Add another place"}</span>
			<Icon icon="ri:arrow-right-s-line" width={18} class="chev" />
		</button>
	</div>
	<p class="fine outside">
		{#if audio.mutedBy === "place"}
			You are at one of these places now. Nothing is being kept.
		{:else if places.length === 0}
			The people you should not record are at a place, not at an hour. Mark it here and the mic keeps nothing while you are inside it.
		{:else}
			Inside these places the mic stays on and keeps nothing. Your record notes the muted stretch, never the place.
		{/if}
	</p>
{/if}

<div class="label">About recording</div>
<p class="fine outside consent">
	The microphone stays on while your phone is with you. It records the sound of your day, and
	everyone in the room. Recordings and transcripts go to your server and nowhere else. Other
	people's voices will be in the record: in some places, recording a conversation needs
	everyone's consent. That part is yours to honor.
</p>

{#if pickerOpen}
	<MobilePlacePicker muted={places} {fix} onDone={pickerDone} onClose={() => (pickerOpen = false)} />
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
	.row.sub {
		padding-left: 28px;
	}
	.editor {
		background: color-mix(in srgb, var(--wash-ink) 3%, transparent);
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
		justify-content: flex-start;
		cursor: default;
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
	.day {
		gap: 8px;
	}
	.day-toggle {
		display: flex;
		align-items: center;
		gap: 8px;
		border: 0;
		background: transparent;
		padding: 0;
		cursor: pointer;
		color: inherit;
	}
	.day-name {
		width: 30px;
	}
	.fine {
		font-size: 12px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
		margin: 0;
		padding: 8px 14px 12px 28px;
	}
	.fine.outside {
		padding: 8px 4px 0;
	}
	.fine.consent {
		font-size: 13px;
		line-height: 1.5;
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
	.switch.small {
		width: 30px;
		height: 18px;
		border-radius: 9px;
	}
	.switch.small::after {
		width: 14px;
		height: 14px;
	}
	.switch.small.on::after {
		transform: translateX(12px);
	}
</style>
