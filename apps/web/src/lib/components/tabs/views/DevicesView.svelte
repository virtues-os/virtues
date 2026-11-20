<!--
  Settings · Your server · Devices (/virtues/devices): everything paired to
  your server, and how each one reaches it.

  One row per thing you'd call a device. A Mac's collector folds into its
  app's row, since together they are one Mac; it lists on its own only when
  its app is no longer paired. An agent lists on its own and names the Mac it
  was added from. Each row opens that device's page, where it is unpaired.

  Below the list: reaching your server away from home (the relay switch,
  which is about how your devices find it), and Start over, which unpairs
  every device at once.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { onMount } from "svelte";
	import { toast } from "svelte-sonner";
	import SettingsPage from "$lib/components/settings/SettingsPage.svelte";
	import NetworkSection from "$lib/components/settings/NetworkSection.svelte";
	import ReopenSection from "$lib/components/settings/ReopenSection.svelte";
	import Button from "$lib/components/Button.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import Modal from "$lib/components/Modal.svelte";
	import DevicePairModal from "$lib/components/sources/DevicePairModal.svelte";
	import AgentConnectModal from "$lib/components/devices/AgentConnectModal.svelte";
	import { listDevices } from "$lib/api/client";
	import { createResource } from "$lib/utils/resource.svelte";
	import { isMacOS } from "$lib/utils/platform";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { lightPendingSetting } from "$lib/settings/light";
	import {
		kindIcon,
		kindSentence,
		deviceName,
		reachLine,
		deviceHref,
		collectorOf,
		isCollector,
		deniedPermissions,
		type Device,
		type DevicesResponse,
	} from "$lib/devices/shared";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	const MANUAL = "https://virtues.com/docs/operate/reach";

	const res = createResource(() => listDevices<DevicesResponse>());
	const devices = $derived(res.data?.devices ?? []);

	onMount(() => lightPendingSetting());

	type Row = { device: Device; line: string; notes: string[] };

	// The device you're using first, then the server's order (last reached
	// first). A collector whose app is listed folds into that app's row.
	const rows = $derived.by((): Row[] => {
		const listed = devices.filter(
			(d) => !(isCollector(d) && d.installed_by && devices.some((p) => p.id === d.installed_by)),
		);
		listed.sort((a, b) => Number(b.is_current) - Number(a.is_current));
		return listed.map((d) => ({ device: d, line: `${kindSentence(d, devices)}. ${reachLine(d)}`, notes: notesFor(d) }));
	});

	/**
	 * What is off about a device, in amber under its line: a permission its
	 * Mac's collector reports as off, a collector that has stopped reporting
	 * or runs behind its app, and a phone your server can't notify.
	 */
	function notesFor(d: Device): string[] {
		const notes: string[] = [];
		const reporter = isCollector(d) ? d : collectorOf(d, devices);
		if (reporter) {
			for (const p of deniedPermissions(reporter)) notes.push(`${p.label} is off, so ${p.costs}.`);
			if (reporter.permissions?.stale) notes.push("Its collector hasn't reported lately. It may have stopped.");
			if (reporter !== d && reporter.version && d.app_version && reporter.version !== d.app_version) {
				notes.push("Its collector runs an older version than the app. Relaunch the app.");
			}
		}
		if (d.kind === "mobile_app" && !d.push_address_at) notes.push("Your server can't send notifications to this phone.");
		return notes;
	}

	function open(d: Device) {
		windowShellStore.navigate(deviceHref(d), { label: d.is_current ? "Settings · This device" : `Settings · ${deviceName(d)}` });
	}

	// Add a device: which kind, then the one pairing sheet Setup and Sources
	// use (DevicePairModal). An AI agent can be added only from the Mac app,
	// which runs its bridge.
	let addOpen = $state(false);
	let pairing = $state<{ deviceType: "ios" | "mac"; displayName: string } | null>(null);
	let connectingAgent = $state(false);

	function choose(kind: "ios" | "mac" | "agent") {
		addOpen = false;
		if (kind === "agent") {
			connectingAgent = true;
			return;
		}
		pairing = kind === "ios" ? { deviceType: "ios", displayName: "iPhone" } : { deviceType: "mac", displayName: "computer" };
	}
</script>

<SettingsPage
	title="Devices"
	lede="Everything paired to your server, and how each one reaches it."
	back={{ route: "/virtues/system", label: "Your server" }}
>
	{#snippet actions()}
		<span id="setting-pair-device"><Button variant="secondary" onclick={() => (addOpen = true)}>Add a device</Button></span>
	{/snippet}

	{#if res.error}
		<div class="load-error" role="alert">
			<p>Your server couldn't list your devices.</p>
			<Button variant="secondary" onclick={() => void res.reload()}>Try again</Button>
		</div>
	{:else if res.loading && !res.data}
		<p class="quiet">Loading your devices…</p>
	{:else if rows.length === 0}
		<p class="quiet">Nothing is paired yet. Run virtues pair on your server, or add a device above.</p>
	{:else}
		<ul class="devices" id="setting-devices">
			{#each rows as row (row.device.id)}
				<li>
					<button type="button" class="device" onclick={() => open(row.device)}>
						<span class="glyph"><Icon icon={kindIcon(row.device.kind)} width="18" /></span>
						<span class="words">
							<span class="name">{deviceName(row.device)}</span>
							<span class="line">{row.line}</span>
							{#each row.notes as note (note)}
								<span class="note">{note}</span>
							{/each}
						</span>
						<svg class="chev" viewBox="0 0 24 24" aria-hidden="true" width="15" height="15">
							<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" />
						</svg>
					</button>
				</li>
			{/each}
		</ul>
	{/if}

	<NetworkSection />
	<ReopenSection />

	<p class="aside">
		The manual's <a href={MANUAL} target="_blank" rel="noopener noreferrer">Reaching your server</a> page explains pairing, the
		relay, and what the relay can and can't see.
	</p>
</SettingsPage>

<Modal open={addOpen} onClose={() => (addOpen = false)} title="Add a device" width="sm">
	<div class="add-choices">
		<button type="button" class="add-choice" onclick={() => choose("ios")}>
			<Icon icon="mynaui:mobile" width="20" />
			<span>An iPhone</span>
		</button>
		<button type="button" class="add-choice" onclick={() => choose("mac")}>
			<Icon icon="mynaui:desktop" width="20" />
			<span>A computer</span>
		</button>
		{#if isMacOS}
			<button type="button" class="add-choice" onclick={() => choose("agent")}>
				<Icon icon="mynaui:sparkles" width="20" />
				<span>An AI agent on this Mac</span>
			</button>
		{/if}
	</div>
</Modal>

<AgentConnectModal open={connectingAgent} onClose={() => (connectingAgent = false)} onConnected={() => void res.reload()} />

{#if pairing}
	<DevicePairModal
		deviceType={pairing.deviceType}
		displayName={pairing.displayName}
		open={true}
		onClose={() => (pairing = null)}
		onSuccess={() => {
			pairing = null;
			toast.success("New device paired");
			void res.reload();
		}}
	/>
{/if}

<style>
	.devices {
		list-style: none;
		margin: 24px 0 0;
		padding: 0;
	}

	.device {
		display: grid;
		grid-template-columns: 20px minmax(0, 1fr) 15px;
		align-items: start;
		gap: 12px;
		width: calc(100% + 24px);
		min-height: 60px;
		margin: 0 -12px;
		padding: 12px;
		box-sizing: border-box;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		font: inherit;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
		transition: background var(--motion-hover-out) var(--ease-premium);
	}

	.device:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
		transition-duration: var(--motion-hover-in);
	}

	.glyph,
	.chev {
		margin-top: 2px;
		color: var(--color-foreground-muted);
	}

	.words {
		min-width: 0;
	}

	.name {
		display: block;
		font-size: 15px;
	}

	.line {
		display: block;
		margin-top: 2px;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.note {
		display: block;
		margin-top: 4px;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-warning);
	}

	.quiet {
		margin: 24px 0 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.load-error {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		margin-top: 24px;
	}

	.load-error p {
		margin: 0;
		font-size: 14px;
		color: var(--color-error);
	}

	.aside {
		margin: 32px 0 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.aside a {
		color: var(--color-primary);
		text-decoration: none;
	}

	.add-choices {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(128px, 1fr));
		gap: 12px;
	}

	.add-choice {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 24px 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		background: var(--color-surface);
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.add-choice:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, var(--color-surface));
	}
</style>
