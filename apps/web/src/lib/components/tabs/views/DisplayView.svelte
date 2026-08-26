<!--
  Display — the screen on the box, and what it shows.

  Three ideas, in order:

  1. THE SCREEN RIGHT NOW — a bezel-true miniature that renders the same
     state the glass does. It is a simulation, not a capture: the box's
     server state is what the kiosk draws from, so mirroring that state IS
     mirroring the screen — with one honest caveat (a wedged kiosk process
     shows stale pixels this mirror cannot see, which is what the Restart
     verb below is for). Everything it renders comes from the REDACTED
     mirror: the settings page may know THAT the panel is showing setup
     words, never what they are — proximity stays the authority.

  2. FACTS — the panel as the server can finally see it (/sys/class/drm),
     the kiosk service's state, and the zoom. No inches anywhere: the 7"
     panel's EDID claims 24", so physical size is never shown or derived.

  3. THE FACE — what the ambient slot wears. Built-ins plus every applet
     that ships a face/. Choosing is immediate and reversible; the
     miniature above is the proof. The interruptions (updating, storage
     fault, button held, setup) outrank any choice made here, and saying so
     on the page is what makes hanging anything feel safe.
-->
<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import { Page, LoadingState, ErrorState } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import {
		getDisplaySettings,
		setDisplayFace,
		setDisplayHours,
		restartDisplay,
		listApplets,
		mintFaceToken,
		type DisplaySettings,
		type Applet,
	} from "$lib/api/client";
	import { backendUrl } from "$lib/config/backend";
	import { toast } from "svelte-sonner";

	let data = $state<DisplaySettings | null>(null);
	let loadError = $state<string | null>(null);
	let applets = $state<Applet[]>([]);
	let poll: ReturnType<typeof setInterval> | null = null;

	async function refresh() {
		try {
			data = await getDisplaySettings();
			loadError = null;
		} catch (e) {
			loadError = e instanceof Error ? e.message : String(e);
		}
	}

	onMount(() => {
		void refresh();
		void listApplets().then(
			(a) => (applets = a.filter((x) => x.has_face)),
			() => {
				/* the shelf just shows its built-ins; the page still works */
			},
		);
		// A live view, at furniture cadence. The glass itself polls at 30s;
		// 10s here keeps the mirror honest without being a load.
		poll = setInterval(refresh, 10_000);
	});
	onDestroy(() => {
		if (poll) clearInterval(poll);
	});

	// ── choosing a face ───────────────────────────────────────────────────
	let saving = $state(false);
	async function choose(face: {
		kind: "builtin" | "applet";
		builtin?: string;
		applet_id?: string;
	}) {
		if (saving) return;
		saving = true;
		try {
			await setDisplayFace(face);
			// The glass notices on its next ambient poll (≤30s); the miniature
			// notices now.
			await refresh();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : String(e));
		} finally {
			saving = false;
		}
	}

	// The select's value: `builtin:<name>` or `applet:<id>`. A select rather
	// than a row list — applets grow unbounded, and a page of radio rows
	// grows with them; a select holds fifty faces as calmly as five.
	const faceValue = $derived(
		data?.face.kind === "applet" && data.face.applet_id
			? `applet:${data.face.applet_id}`
			: `builtin:${data?.face.builtin ?? "record"}`,
	);
	// A hung face whose applet has since vanished still needs an option to
	// sit on, or the select silently shows the wrong thing.
	const orphanedAppletId = $derived.by(() => {
		const f = data?.face;
		if (f?.kind !== "applet" || !f.applet_id) return null;
		return applets.some((a) => a.id === f.applet_id) ? null : f.applet_id;
	});
	function onFaceChange(value: string) {
		const sep = value.indexOf(":");
		const kind = value.slice(0, sep);
		const rest = value.slice(sep + 1);
		if (kind === "applet") void choose({ kind: "applet", applet_id: rest });
		else void choose({ kind: "builtin", builtin: rest });
	}

	// ── hours ─────────────────────────────────────────────────────────────
	// Local buffers, seeded once from the box — the 10s refresh must not
	// clobber a half-picked time. Saved when both are set; cleared as a pair.
	let sleepStart = $state("");
	let sleepEnd = $state("");
	let hoursSeeded = $state(false);
	$effect(() => {
		if (hoursSeeded || !data) return;
		sleepStart = data.hours.sleep_start?.slice(0, 5) ?? "";
		sleepEnd = data.hours.sleep_end?.slice(0, 5) ?? "";
		hoursSeeded = true;
	});

	let savingHours = $state(false);
	async function saveHours(start: string | null, end: string | null) {
		if (savingHours) return;
		savingHours = true;
		try {
			await setDisplayHours({ sleep_start: start, sleep_end: end });
			await refresh();
		} catch (e) {
			toast.error(e instanceof Error ? e.message : String(e));
		} finally {
			savingHours = false;
		}
	}
	function onHoursChange() {
		// A complete pair is a schedule; BOTH blanked is "never sleeps" —
		// without that, clearing the fields left the box silently keeping
		// the old hours behind an empty-looking form. A lone time just
		// waits for its partner.
		if (sleepStart && sleepEnd) void saveHours(sleepStart, sleepEnd);
		else if (!sleepStart && !sleepEnd && hoursSet) void saveHours(null, null);
	}
	function clearHours() {
		sleepStart = "";
		sleepEnd = "";
		void saveHours(null, null);
	}
	const hoursSet = $derived(
		Boolean(data?.hours.sleep_start && data?.hours.sleep_end),
	);
	const wakeLabel = $derived(data?.hours.sleep_end?.slice(0, 5) ?? "");

	// ── restart ───────────────────────────────────────────────────────────
	let restarting = $state(false);
	async function restart() {
		if (restarting) return;
		restarting = true;
		try {
			await restartDisplay();
			toast.success("The screen is restarting.");
		} catch (e) {
			toast.error(e instanceof Error ? e.message : String(e));
		} finally {
			restarting = false;
		}
	}

	// ── the miniature ─────────────────────────────────────────────────────
	// When the configured face is an applet's, the miniature hangs the real
	// face — same iframe jail as everywhere else — scaled from the panel's
	// 585×329 canvas to whatever width the page gives us.
	let glassW = $state(0);
	const miniAppletId = $derived(
		data?.state.claimed && data.face.kind === "applet"
			? (data.face.applet_id ?? null)
			: null,
	);
	let miniSrc = $state<string | null>(null);
	// Face tokens live one hour; a Settings tab left open longer would keep
	// an iframe whose every query fails. Re-mint inside the TTL, same as the
	// kiosk page does.
	const MINI_REMINT_MS = 45 * 60 * 1000;
	$effect(() => {
		const id = miniAppletId;
		if (!id) {
			miniSrc = null;
			return;
		}
		let cancelled = false;
		const mint = async () => {
			try {
				const { token } = await mintFaceToken(id);
				if (cancelled) return;
				miniSrc = backendUrl(
					`/face/${encodeURIComponent(id)}/?vt=${encodeURIComponent(token)}&theme=dark&surface=panel`,
				);
			} catch {
				if (!cancelled) miniSrc = null;
			}
		};
		void mint();
		const t = setInterval(mint, MINI_REMINT_MS);
		return () => {
			cancelled = true;
			clearInterval(t);
		};
	});

	const firstRecordLine = $derived.by(() => {
		const rec = data?.state.record ?? [];
		if (!rec.length) return null;
		return `${rec[0].count.toLocaleString()} ${rec[0].label}`;
	});

	// ── facts ─────────────────────────────────────────────────────────────
	const modeLabel = $derived.by(() => {
		const p = data?.panel;
		if (!p) return null;
		if (p.mode_width && p.mode_height) return `${p.mode_width} × ${p.mode_height}`;
		return p.connector;
	});
	const zoomLabel = $derived.by(() => {
		if (data?.zoom_override != null)
			return `${data.zoom_override} — set by hand (VIRTUES_DISPLAY_ZOOM)`;
		if (data?.zoom_derived != null)
			return `${data.zoom_derived.toFixed(2)} — derived from the panel`;
		return null;
	});
</script>

<Page
	title="Display"
	description="The screen on the box, and what it shows."
	maxWidth="wide"
>
	{#if !data && !loadError}
		<LoadingState />
	{:else if !data}
		<ErrorState message={loadError ?? "Couldn't reach the box."} onRetry={refresh} />
	{:else}
		<div class="display-settings">
			{#if !data.attached}
				<!-- A DIY box, or a dev checkout. The room stays open — the face
				     choice keeps, and a screen picks it up when one arrives —
				     but it says so rather than implying hardware that isn't
				     there. -->
				<p class="notice">
					<Icon icon="ri:information-line" width="14" />
					No screen is attached to this box. These settings keep; a screen
					uses them when one arrives.
				</p>
			{/if}

			<section>
				<header class="sec-head">
					<h2 class="section-title">The screen right now</h2>
					{#if data.unit_state !== "not installed"}
						<button
							class="ghost-btn"
							onclick={restart}
							disabled={restarting}
						>
							<Icon
								icon={restarting ? "ri:loader-4-line" : "ri:restart-line"}
								width="14"
							/>
							<span>{restarting ? "Restarting…" : "Restart the screen"}</span>
						</button>
					{/if}
				</header>

				<!-- The glass, in miniature. Literal panel colors, not theme
				     tokens — this is a picture of a specific dark screen, and it
				     must look the same in all sixteen themes. -->
				<div class="glass" bind:clientWidth={glassW}>
					{#if data.state.updating}
						<div class="g-fault">
							<span class="g-doing">Updating</span>
							<span class="g-sub">Back in a minute. Don't unplug me.</span>
						</div>
					{:else if data.state.data_disk_fault}
						<div class="g-fault">
							<span class="g-doing">Storage disconnected</span>
							<span class="g-sub">{data.state.data_disk_fault}</span>
						</div>
					{:else if !data.state.claimed}
						<div class="g-fault">
							<span class="g-doing">Get Virtues for your computer</span>
							<!-- The words themselves never leave the box — the mirror
							     may say the panel is showing them, not what they are. -->
							<span class="g-sub">Showing the setup words — readable only on the glass itself.</span>
						</div>
					{:else if data.state.asleep}
						<!-- Mirroring blackness would read as a fault; the mirror
						     says what the darkness is. -->
						<div class="g-fault">
							<span class="g-doing">Asleep</span>
							<span class="g-sub">
								Backlight off{wakeLabel ? ` until ${wakeLabel}` : ""}. The
								screen still wakes for anything that matters.
							</span>
						</div>
					{:else if data.face.kind === "builtin" && data.face.builtin === "matte"}
						<div class="g-matte"></div>
					{:else if miniAppletId && miniSrc}
						<div
							class="g-scaler"
							style:transform={`scale(${glassW > 0 ? glassW / 585 : 1})`}
						>
							<iframe
								src={miniSrc}
								sandbox="allow-scripts"
								title="The face on the screen"
							></iframe>
						</div>
					{:else}
						<div class="g-amb">
							<div class="g-top">
								<span class="g-name">∴ {data.state.box_name}</span>
								<span class="g-status" class:g-off={!data.state.online}>
									{data.state.online ? "REACHABLE" : "OFFLINE"}
								</span>
							</div>
							<div class="g-log">
								<span class="g-kicker">THE RECORD</span>
								<span class="g-line">
									{firstRecordLine ?? "Nothing has arrived yet."}
								</span>
								<span class="g-meta">
									{data.state.devices}
									{data.state.devices === 1 ? "device" : "devices"} syncing
								</span>
							</div>
						</div>
					{/if}
				</div>

				<!-- The duty list. Disclosure, not configuration: the one
				     standing answer to "why did my face disappear". -->
				<p class="duty">
					The screen interrupts any face for: updating &middot; storage fault
					&middot; button held &middot; setup.
				</p>

				<dl class="facts">
					<dt>Panel</dt>
					<dd>{modeLabel ?? "None detected"}</dd>
					<dt>Service</dt>
					<dd>{data.unit_state}</dd>
					{#if zoomLabel}
						<dt>Zoom</dt>
						<dd>{zoomLabel}</dd>
					{/if}
				</dl>
			</section>

			<section>
				<h2 class="section-title">The face</h2>
				<!-- A SELECT, NOT A ROW LIST — same reasoning as the channel
				     picker: it has a default, it scales to any number of applet
				     faces, and the miniature above is the preview. -->
				<div class="face-picker">
					<select
						value={faceValue}
						disabled={saving}
						onchange={(e) => onFaceChange(e.currentTarget.value)}
					>
						<option value="builtin:record">The Record — the census, ticking</option>
						<option value="builtin:matte">Matte — black glass, on purpose</option>
						{#if applets.length || orphanedAppletId}
							<optgroup label="Applet faces">
								{#each applets as applet (applet.id)}
									<option value={`applet:${applet.id}`}>{applet.name}</option>
								{/each}
								{#if orphanedAppletId}
									<option value={`applet:${orphanedAppletId}`}>
										{orphanedAppletId} (missing)
									</option>
								{/if}
							</optgroup>
						{/if}
					</select>
				</div>
				<p class="sec-hint">
					Changes reach the screen within half a minute. Applets with a face
					appear in the list.
				</p>
			</section>

			<section>
				<h2 class="section-title">Hours</h2>
				<!-- Two times ARE the whole data model; anything fancier is a
				     considered-looking widget doing no extra work. What they
				     control is real power — the backlight goes off with the
				     signal (backlight audit, docs/display-plan.md). -->
				<div class="hours-row">
					<label class="hours-field">
						Sleeps at
						<input
							type="time"
							bind:value={sleepStart}
							disabled={savingHours}
							onchange={onHoursChange}
						/>
					</label>
					<label class="hours-field">
						Wakes at
						<input
							type="time"
							bind:value={sleepEnd}
							disabled={savingHours}
							onchange={onHoursChange}
						/>
					</label>
					{#if hoursSet}
						<button class="ghost-btn" onclick={clearHours} disabled={savingHours}>
							Never sleeps
						</button>
					{/if}
				</div>
				<p class="sec-hint">
					The screen goes truly dark — backlight off — and still wakes for
					anything on the duty list. Leave empty and it never sleeps.
				</p>
			</section>

			<section>
				<h2 class="section-title">Other screens</h2>
				<p class="sec-hint">
					Any paired device can show the face: open <code>/display</code> in
					its browser and go full screen.
				</p>
			</section>
		</div>
	{/if}
</Page>

<style>
	.display-settings {
		display: flex;
		flex-direction: column;
		gap: 32px;
	}

	.notice {
		display: flex;
		align-items: center;
		gap: 6px;
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	section {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.sec-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.section-title {
		font-size: 14px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		margin: 0;
	}

	.sec-hint {
		margin: 0;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		max-width: 60ch;
	}
	.sec-hint code {
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		padding: 1px 4px;
		border-radius: 4px;
		background: var(--color-surface-elevated);
	}

	.ghost-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 4px 8px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: none;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.ghost-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground);
	}
	.ghost-btn:disabled {
		opacity: 0.55;
		cursor: default;
	}

	/* ── the miniature ── */
	/* 585:329 is the panel's canvas; at full width on a wide page the mirror
	   is true-size. Literal colors from the kiosk page, deliberately. */
	.glass {
		position: relative;
		width: min(585px, 100%);
		aspect-ratio: 585 / 329;
		background: #0b0f14;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		overflow: hidden;
	}
	.g-fault,
	.g-amb {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		justify-content: center;
		padding: 0 40px;
		box-sizing: border-box;
	}
	.g-doing {
		font-family: Georgia, "Liberation Serif", serif;
		font-size: 1.2rem;
		color: #f5f2ec;
		margin-bottom: 4px;
	}
	.g-sub {
		font-size: 0.75rem;
		line-height: 1.45;
		color: #7d8b99;
		max-width: 420px;
	}
	.g-matte {
		position: absolute;
		inset: 0;
		background: #000;
	}
	.g-scaler {
		position: absolute;
		top: 0;
		left: 0;
		width: 585px;
		height: 329px;
		transform-origin: top left;
	}
	.g-scaler iframe {
		width: 585px;
		height: 329px;
		border: 0;
		/* A picture of the screen, not the screen — the panel has no touch,
		   and neither does its mirror. */
		pointer-events: none;
	}
	.g-amb {
		justify-content: space-between;
		padding: 14px 28px 12px;
	}
	.g-top {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
	}
	.g-name {
		font-family: Georgia, "Liberation Serif", serif;
		font-size: 0.85rem;
		color: #7d8b99;
	}
	.g-status {
		font-family: ui-monospace, Menlo, monospace;
		font-size: 0.55rem;
		letter-spacing: 0.1em;
		color: #5fb07e;
	}
	.g-status.g-off {
		color: #c9a227;
	}
	.g-log {
		display: flex;
		flex-direction: column;
		gap: 8px;
		border-top: 1px solid #1b242e;
		padding-top: 10px;
		flex: 1;
		justify-content: center;
	}
	.g-kicker {
		font-family: ui-monospace, Menlo, monospace;
		font-size: 0.55rem;
		letter-spacing: 0.11em;
		color: #4a5663;
	}
	.g-line {
		font-size: 1.1rem;
		color: #f5f2ec;
	}
	.g-meta {
		font-size: 0.65rem;
		color: #4a5663;
	}

	.duty {
		margin: 0;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.facts {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 8px 16px;
		align-items: center;
		margin: 0;
		font-size: 13px;
	}
	.facts dt {
		color: var(--color-foreground-subtle);
	}
	.facts dd {
		margin: 0;
	}

	/* ── hours ── */
	.hours-row {
		display: flex;
		align-items: center;
		gap: 16px;
		flex-wrap: wrap;
	}
	.hours-field {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.hours-field input {
		font: inherit;
		font-size: 13px;
		padding: 4px 8px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-background);
		color: var(--color-foreground);
	}

	/* ── the picker ── */
	.face-picker select {
		font: inherit;
		font-size: 13px;
		padding: 4px 8px;
		border-radius: 6px;
		border: 1px solid var(--color-border);
		background: var(--color-background);
		color: var(--color-foreground);
	}
</style>
