<!--
  A Mac's permissions: Full Disk Access (required: Messages and Safari
  history), Accessibility (optional: window titles). The one place these rows
  are drawn, so Setup, the devices list and a device's page cannot disagree
  about what a permission is called, what it costs, or where to turn it on.

  Two readings, never merged:
    - `status`: the collector running on THIS Mac (`get_collector_status`).
      Only the daemon's own report counts; see `macPermissionsFromStatus`.
    - `device`: what a collector last reported to the server. Pass the
      COLLECTOR's row (`collectorOf`), not the app's: only the collector
      reports permissions.

  macOS sends nothing when a toggle flips, so with `onRecheck` the panel asks
  again when the window comes back into focus, which is the moment someone
  returns from System Settings, and offers "Check again" by hand.

  On a shell with the drag tile (PERMISSION_TILE_SURFACE), "Open …" also puts
  a tile on System Settings holding the collector to drag into the list. The
  person stays in System Settings, so the panel asks every few seconds while
  the tile is up, and closes it once the grant lands, which brings Virtues
  back to the front.

  The wording lives in `$lib/devices/shared.ts` (PERMISSION_COPY).
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { Button, TextAction } from "$lib";
	import { onMount } from "svelte";
	import {
		closePermissionTile,
		shellSupports,
		PERMISSION_TILE_SURFACE,
		type CollectorStatus,
	} from "$lib/tauri/bridge";
	import {
		macPermissionsFromDevice,
		macPermissionsFromStatus,
		type Device,
		type MacPermission,
	} from "$lib/devices/shared";

	interface Props {
		/** The local collector's status. Present (even null, while loading) means "this Mac". */
		status?: CollectorStatus | null;
		/** A device row's reported permissions, when not standing on that Mac. */
		device?: Pick<Device, "permissions"> | null;
		/** Show the "Open …" buttons. Defaults to true for a local reading. */
		canOpen?: boolean;
		/** Draw only what is off, as warnings. For a list row. */
		deniedOnly?: boolean;
		/** Ask the collector to check again (`recheckCollector`). Local only. */
		onRecheck?: () => unknown;
	}

	let { status, device, canOpen, deniedOnly = false, onRecheck }: Props = $props();

	const local = $derived(status !== undefined);
	const openable = $derived(canOpen ?? local);
	const all = $derived<MacPermission[]>(
		local ? macPermissionsFromStatus(status ?? null) : macPermissionsFromDevice(device ?? null),
	);
	const rows = $derived(deniedOnly ? all.filter((p) => p.granted === false) : all);
	const missing = $derived(all.some((p) => p.granted !== true));

	let checking = $state(false);
	async function recheck() {
		if (!onRecheck || checking) return;
		checking = true;
		try {
			await onRecheck();
		} finally {
			checking = false;
		}
	}

	/** The shell is new enough to show the drag tile when a pane opens. */
	let hasTile = $state(false);
	/** The last "Open …" opened only the pane, so the drag copy would be wrong. */
	let noTileShown = $state(false);
	/** A tile this panel opened may still be on System Settings. */
	let tileOpen = false;
	/** The permission whose grant the panel is watching for while its tile is up. */
	let watching = $state<MacPermission["key"] | null>(null);
	// MAX_LIFE in permission_tile.rs is a little longer than this.
	const WATCH_EVERY_MS = 4_000;
	const WATCH_FOR_MS = 3 * 60_000;
	let watchTimer: ReturnType<typeof setInterval> | undefined;

	function stopWatching() {
		clearInterval(watchTimer);
		watchTimer = undefined;
		watching = null;
	}

	function closeTile(returnToApp: boolean) {
		stopWatching();
		if (!tileOpen) return;
		tileOpen = false;
		void closePermissionTile(returnToApp);
	}

	function watch(key: MacPermission["key"]) {
		stopWatching();
		tileOpen = true;
		watching = key;
		const started = Date.now();
		watchTimer = setInterval(() => {
			if (Date.now() - started > WATCH_FOR_MS) return closeTile(false);
			void recheck();
		}, WATCH_EVERY_MS);
	}

	$effect(() => {
		if (!watching) return;
		if (all.find((p) => p.key === watching)?.granted === true) closeTile(true);
	});

	onMount(() => {
		if (!local || !onRecheck) return;
		void shellSupports(PERMISSION_TILE_SURFACE).then((yes) => (hasTile = yes));
		const onFocus = () => {
			// Back in Virtues: the tile has done its job or been left behind.
			closeTile(false);
			if (missing) void recheck();
		};
		window.addEventListener("focus", onFocus);
		return () => {
			window.removeEventListener("focus", onFocus);
			closeTile(false);
		};
	});

	async function open(e: MouseEvent, perm: MacPermission) {
		// Rows can sit inside a clickable list row; this must not also open it.
		e.stopPropagation();
		const opened = await perm.open?.();
		noTileShown = opened === "pane";
		if (opened === "tile" && onRecheck) watch(perm.key);
	}

	function icon(p: MacPermission) {
		if (p.granted === true) return "mynaui:check-circle";
		if (p.granted === false) return p.required ? "mynaui:danger-circle" : "mynaui:info-circle";
		return "mynaui:question-circle";
	}
</script>

{#if rows.length}
	<ul>
		{#each rows as perm (perm.key)}
			<li
				class={`flex items-start gap-3 ${
					deniedOnly
						? "mt-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2"
						: "p-4"
				}`}
			>
				<Icon
					icon={icon(perm)}
					class={`flex-none mt-0.5 ${
						perm.granted === true
							? "text-success"
							: perm.granted === false && perm.required
								? "text-warning"
								: "text-foreground-muted"
					}`}
				/>
				<div class="flex-1 min-w-0">
					<div class="text-sm text-foreground">
						{perm.label}
						{#if !perm.required}
							<span class="text-foreground-subtle"> · optional</span>
						{/if}
					</div>
					{#if perm.granted === true && perm.reads}
						<div class="text-xs text-foreground-muted mt-0.5">On. {perm.reads}.</div>
					{:else if perm.granted === false}
						<div class="text-xs text-foreground-muted mt-0.5">Off, so {perm.costs}.</div>
						{#if openable && perm.open}
							<div class="text-xs text-foreground-muted mt-1">
								{hasTile && !noTileShown && perm.dragFix ? perm.dragFix : perm.fix}
							</div>
							<div class="mt-2">
								<Button variant="secondary" onclick={(e) => open(e, perm)}>
									Open {perm.pane}{local ? "" : " on this Mac"}
								</Button>
							</div>
						{:else if !openable}
							<div class="text-xs text-foreground-muted mt-0.5">
								Someone at that Mac has to turn this on. macOS has no way to grant it remotely.
							</div>
						{/if}
					{:else if perm.granted === null}
						<div class="text-xs text-foreground-muted mt-0.5">Not reported yet</div>
					{/if}
				</div>
			</li>
		{/each}
	</ul>
	{#if local && onRecheck && missing && !deniedOnly}
		<div class="px-4 pb-4">
			<TextAction onclick={recheck} loading={checking} loadingLabel="Checking…">Check again</TextAction>
		</div>
	{/if}
{/if}

