<script lang="ts">
	import { onMount } from "svelte";
	import { updated } from "$app/state";
	import {
		appUpdateState,
		applyAppUpdate,
		bundleUpdateReady,
		runsOwnCopy,
		shellIdentity,
		stagedBundle,
	} from "$lib/tauri/bridge";
	import { onBoxBuildChanged } from "$lib/build";
	import { reloadApp, uploadsHold } from "$lib/reload";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// A staged app update, waiting for a relaunch. The shell stages silently
	// (the Chrome model) and applies on the next launch — but this app is a
	// menu-bar resident designed never to relaunch, so "next launch" rounds to
	// never and the only surface that knew was the tray. This chip is the
	// in-app door: quiet, standing, one click. Null everywhere the
	// self-updater doesn't exist (browser, phone, Windows/Linux), so it simply
	// never renders there.
	let stagedVersion = $state<string | null>(null);

	// ── The OTHER update track ───────────────────────────────────────────────
	//
	// The UI moves separately from the app, and a menu-bar resident that is
	// never relaunched keeps showing the old one until something reloads it.
	// Where the page comes from decides how to tell:
	//
	// - The app's own copy (`virtues://`, the Mac and the phone): the shell
	//   downloads the box's bundle and stages it, and this page runs the one it
	//   loaded from. SvelteKit's `updated` would compare against the app's own
	//   origin, which always matches itself, so the shell answers instead:
	//   `bundleUpdateReady()` is true when a bundle newer than this page waits.
	// - The box's copy (a browser, the Windows and Linux apps): SvelteKit bakes
	//   a build id into this bundle and the box publishes the deployed one at
	//   `/_app/version.json`, so `updated.current` is the comparison. It is
	//   bundle to bundle, so a reload converges by construction. Not `/health`:
	//   that reports the box BINARY's commit, which drifts from the served UI
	//   on purpose (`virtues upgrade --only web`, a hand-built box).
	const ownCopy = runsOwnCopy();
	let bundleStaged = $state(false);
	const uiStale = $derived(ownCopy ? bundleStaged : updated.current);

	// The offered build's identity, for the chip's second line: the staged
	// bundle's hash in the app's own copy, and otherwise the sha from
	// `/api/web-bundle/version`, which describes the build the box is SERVING.
	// Fetched only when the chip is about to show, because that is the only
	// time anyone reads it.
	let distSha = $state<string | null>(null);

	async function pollUpdate() {
		const s = await appUpdateState();
		stagedVersion = s?.stagedVersion ?? null;

		if (ownCopy) {
			bundleStaged = await bundleUpdateReady();
			const shell = bundleStaged ? await shellIdentity() : null;
			const staged = shell ? stagedBundle(shell) : null;
			distSha = staged ? staged.slice(0, 8) : null;
			return;
		}

		// No try/catch: `check()` answers `false` on every failure path rather
		// than throwing — non-2xx, unparseable, offline — so a dropped poll or a
		// box mid-upgrade cannot flap the chip, and a box with no version.json
		// at all (headless, or a dev box serving from vite) cannot pin it open.
		// It is also hardwired to `false` in a dev build, which is what the old
		// hand-rolled `!== "dev"` guard was for.
		await updated.check();
		if (!updated.current) return;
		try {
			const res = await fetch("/api/web-bundle/version", { cache: "no-store" });
			// 404 is an ordinary answer here (a dev box serving from vite, a
			// headless install): the chip simply shows no identity line.
			distSha = res.ok ? ((await res.json())?.sha ?? null) : null;
		} catch {
			distSha = null;
		}
	}

	/** Relaunch into the staged release, or take the chip down if there is none. */
	async function relaunch() {
		// A file still on its way to the server would end with this page.
		if (uploadsHold("relaunch", () => void applyAppUpdate())) return;
		if (await applyAppUpdate()) return; // unreachable: the app restarts
		// The shell had nothing staged after all. Rather than leave a button
		// that does nothing when pressed, drop the chip and re-read the state
		// that produced it.
		stagedVersion = null;
		void pollUpdate();
	}

	onMount(() => {
		void pollUpdate();
		// The shell checks every 6h; ten minutes keeps the chip honest without
		// chatter on an IPC call that answers from memory.
		const updateId = setInterval(pollUpdate, 600_000);
		// A box that restarts under us is worth knowing about before the next
		// tick of that timer — ten minutes of a page quietly holding chunks the
		// box no longer has is ten minutes too many. See $lib/build.
		const offBoxMoved = onBoxBuildChanged(() => void pollUpdate());
		return () => {
			clearInterval(updateId);
			offBoxMoved();
		};
	});

	// Sources and Developer are headings in Settings, and Settings is the
	// row above this footer. A second door to either here would be the same
	// destination twice on one screen.
	//
	// There is no "Sign Out" — auth is the device's proven iroh key, not a
	// server session; to drop this device use Settings → Devices → Unpair.
</script>

<div
	class="footer"
	class:collapsed
	style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
>
	<!-- TWO tracks, and each gets its own chip when it has something to say.
	     This was one chip with the app update winning outright, on the argument
	     that relaunching also reloads the UI — true, and still the reason it is
	     listed first. What the argument missed is that it only holds if the
	     relaunch HAPPENS. A staged release is set the moment the background
	     updater stages it and clears only when the process restarts, and this is
	     a menu-bar resident designed never to restart — the same premise the
	     chip below is built on. So a staged update parks in that slot for days
	     and silently blanks the other track for the whole time.

	     Neither ever acts on its own: an update that interrupts what you were
	     typing is worse than an update that waits. -->
	{#if stagedVersion && !collapsed}
		<button
			type="button"
			class="relaunch"
			onclick={() => void relaunch()}
			data-tip="Restart into the downloaded update - takes a few seconds"
		>
			<span class="relaunch-label">Relaunch to update</span>
			<span class="relaunch-version">v{stagedVersion}</span>
		</button>
	{/if}
	{#if uiStale && !collapsed}
		<button
			type="button"
			class="relaunch"
			onclick={() => reloadApp()}
			data-tip={ownCopy
				? "This app downloaded a newer interface - reload to switch to it"
				: "Your server is serving a newer interface - reload to pick it up"}
		>
			<span class="relaunch-label">Reload for the latest</span>
			{#if distSha}<span class="relaunch-version">{distSha}</span>{/if}
		</button>
	{/if}


</div>

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.footer {
		@apply flex flex-col gap-1 py-3 mt-auto;
		padding-left: 8px;
		/* Staggered load animation (initial mount) */
		animation: sidebar-fade-slide-in var(--motion-enter) var(--ease-premium) backwards;
		/* Staggered expand transition - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity var(--motion-enter) var(--ease-premium) var(--stagger-delay, 0ms),
			transform var(--motion-enter) var(--ease-premium) var(--stagger-delay, 0ms);
	}

	/* With no chip to show, the footer takes no room, so Settings sits at the
	   foot of the sidebar instead of 24px above it. */
	.footer:not(:has(> *)) {
		padding-block: 0;
	}

	/* The update chip: a standing offer, not an alarm. Info tokens (note-this),
	   never the accent (act-on-this is the doors' register) — the split the
	   theme pass argued for. Reads like a Library row that happens to carry a
	   second, dim line. */
	.relaunch {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 1px;
		width: 100%;
		padding: 6px var(--sidebar-interactive-padding);
		margin-bottom: 4px;
		border: 1px solid var(--color-info-subtle);
		border-radius: var(--sidebar-interactive-radius);
		background: var(--color-info-subtle);
		cursor: pointer;
		text-align: left;
	}

	.relaunch-label {
		font-size: var(--chrome-label-font-size);
		color: var(--color-info);
	}

	.relaunch-version {
		font-family: var(--font-mono);
		font-size: 11px;
		letter-spacing: 0.07em;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	.relaunch:hover {
		border-color: var(--color-info);
	}

	.relaunch:focus-visible {
		outline: 2px solid var(--color-info);
		outline-offset: -2px;
	}

	.footer.collapsed {
		@apply items-center;
		padding-left: 4px;
		padding-right: 4px;
		opacity: 0;
		transition:
			opacity var(--motion-state) var(--ease-premium),
			transform var(--motion-state) var(--ease-premium);
	}
</style>
