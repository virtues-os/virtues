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

	const isBuiltin = (name: string) =>
		data?.face.kind === "builtin" && data.face.builtin === name;
	const isApplet = (id: string) =>
		data?.face.kind === "applet" && data.face.applet_id === id;

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
	$effect(() => {
		const id = miniAppletId;
		if (!id) {
			miniSrc = null;
			return;
		}
		let cancelled = false;
		void (async () => {
			try {
				const { token } = await mintFaceToken(id);
				if (cancelled) return;
				miniSrc = backendUrl(
					`/face/${encodeURIComponent(id)}/?vt=${encodeURIComponent(token)}&theme=dark&surface=panel`,
				);
			} catch {
				if (!cancelled) miniSrc = null;
			}
		})();
		return () => {
			cancelled = true;
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
				<p class="sec-hint">
					What the screen shows once the box is claimed and nothing needs
					saying. Changes land on the glass within half a minute.
				</p>

				<ul class="shelf">
					<li>
						<button
							class="face-row"
							class:selected={isBuiltin("record")}
							disabled={saving}
							onclick={() => choose({ kind: "builtin", builtin: "record" })}
						>
							<span class="face-name">The Record</span>
							<span class="face-desc">
								The census of the record, ticking — what every box shows out
								of the box.
							</span>
							{#if isBuiltin("record")}
								<Icon icon="ri:check-line" width="16" class="face-check" />
							{/if}
						</button>
					</li>
					<li>
						<button
							class="face-row"
							class:selected={isBuiltin("matte")}
							disabled={saving}
							onclick={() => choose({ kind: "builtin", builtin: "matte" })}
						>
							<span class="face-name">Matte</span>
							<span class="face-desc">
								Black glass, on purpose. The screen still speaks up for
								anything that matters.
							</span>
							{#if isBuiltin("matte")}
								<Icon icon="ri:check-line" width="16" class="face-check" />
							{/if}
						</button>
					</li>
					{#each applets as applet (applet.id)}
						<li>
							<button
								class="face-row"
								class:selected={isApplet(applet.id)}
								disabled={saving}
								onclick={() => choose({ kind: "applet", applet_id: applet.id })}
							>
								<span class="face-name">{applet.name}</span>
								{#if applet.description}
									<span class="face-desc">{applet.description}</span>
								{/if}
								{#if isApplet(applet.id)}
									<Icon icon="ri:check-line" width="16" class="face-check" />
								{/if}
							</button>
						</li>
					{/each}
				</ul>

				<!-- The door at the end of the shelf: faces are chat-authored
				     today (applet_setup's face_html), so the invitation is real,
				     not aspirational. -->
				<p class="sec-hint">
					Any applet with a face can hang here. Ask the assistant for a new
					one — a small page over your own record, made for a 585 × 329
					screen that nobody touches.
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

	/* ── the shelf ── */
	.shelf {
		list-style: none;
		margin: 0;
		padding: 0;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		background: var(--color-surface);
		overflow: hidden;
	}
	.shelf li + li {
		border-top: 1px solid var(--color-border-subtle, var(--color-border));
	}
	.face-row {
		display: grid;
		grid-template-columns: 1fr auto;
		grid-template-areas:
			"name check"
			"desc check";
		row-gap: 2px;
		column-gap: 12px;
		align-items: center;
		width: 100%;
		text-align: left;
		padding: 12px 14px;
		border: 0;
		background: none;
		cursor: pointer;
		font: inherit;
		color: var(--color-foreground);
	}
	.face-row:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.face-row:disabled {
		cursor: default;
	}
	.face-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}
	.face-name {
		grid-area: name;
		font-size: 13px;
		font-weight: 500;
	}
	.face-desc {
		grid-area: desc;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	.face-row :global(.face-check) {
		grid-area: check;
		color: var(--color-primary);
	}
</style>
