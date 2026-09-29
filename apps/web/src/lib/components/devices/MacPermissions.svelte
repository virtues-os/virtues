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

  The wording lives in `$lib/devices/shared.ts` (PERMISSION_COPY).
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { Button, TextAction } from "$lib";
	import { onMount } from "svelte";
	import type { CollectorStatus } from "$lib/tauri/bridge";
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

	onMount(() => {
		if (!local || !onRecheck) return;
		const onFocus = () => {
			if (missing) void recheck();
		};
		window.addEventListener("focus", onFocus);
		return () => window.removeEventListener("focus", onFocus);
	});

	function open(e: MouseEvent, perm: MacPermission) {
		// Rows can sit inside a clickable list row; this must not also open it.
		e.stopPropagation();
		void perm.open?.();
	}

	function icon(p: MacPermission) {
		if (p.granted === true) return "ri:checkbox-circle-line";
		if (p.granted === false) return p.required ? "ri:error-warning-line" : "ri:information-line";
		return "ri:question-line";
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
					{#if perm.granted === false}
						<div class="text-xs text-foreground-muted mt-0.5">Off, so {perm.costs}.</div>
						{#if openable && perm.open}
							<div class="text-xs text-foreground-muted mt-1">{perm.fix}</div>
							<div class="mt-2">
								<Button variant="secondary" size="sm" onclick={(e) => open(e, perm)}>
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

