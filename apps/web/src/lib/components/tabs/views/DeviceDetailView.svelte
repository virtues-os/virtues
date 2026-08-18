<!--
  One paired device, as the box knows it.

  DELIBERATELY THIN, and the thinness is the honest part. The rich screens you
  see for the device in your hand — MobileDeviceScreen on iOS, ThisMacView on a
  Mac — are local instrument panels: every stream status, queue depth, outbox
  stat and radio reading comes from a Tauri plugin or the collector daemon over
  local IPC, and none of it is ever sent to the box. What the box holds per
  device is `DeviceListItem`: identity, when it was paired, when it last spoke,
  its build, and whatever permissions it self-reported.

  So a page for someone else's iPhone, opened from a Mac, shows those fields and
  stops. That is not a permission we withheld — it is the whole of what crossed
  the wire. Saying so on the page beats rendering empty panels that look broken.

  The other direction is a real project (devices heartbeat their panel up to the
  box, and the box grows a `machine` that credentials hang off, which is also
  what would fix one Mac appearing as two rows). Not this change.
-->
<script lang="ts">
	import { Page, Badge, Button, LoadingState, ErrorState, EmptyState } from '$lib';
	import Icon from '$lib/components/Icon.svelte';
	import { createResource } from '$lib/utils/resource.svelte';
	import { formatTimeAgo } from '$lib/utils/dateUtils';
	import { listDevices } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { isTauri } from '$lib/utils/platform';
	import {
		kindLabel,
		kindIcon,
		deniedPermissions,
		grantedPermissions,
		revokeDeviceFlow,
		backToDevices,
		type Device,
		type DevicesResponse
	} from '$lib/devices/shared';

	// No `tab`/`active`: this view is a pure function of the id in the route and
	// polls nothing, so it has no use for the tab or for knowing it is focused.
	let { deviceId }: { deviceId: string } = $props();

	const res = createResource(() => listDevices<DevicesResponse>());
	const devices = $derived(res.data?.devices ?? []);

	// `this` is a stable alias for whichever row is making the request, so
	// Sources (and the sidebar, and a bookmark) can point at "this machine"
	// without knowing an id that changes on every re-pair.
	const device = $derived<Device | null>(
		deviceId === 'this'
			? (devices.find((d) => d.is_current) ?? null)
			: (devices.find((d) => d.id === deviceId) ?? null)
	);

	const denied = $derived(device ? deniedPermissions(device) : []);
	const granted = $derived(device ? grantedPermissions(device) : []);

	// Same relaxation as the list, for the same reason: it is the COLLECTOR row
	// that carries a denial, and that row is never `is_current`. Gating the fix
	// button on the row being current shipped it inert once already.
	const canFix = $derived(isTauri);

	function openSource(sourceId: string) {
		windowShellStore.navigate(`/sources/${sourceId}`, { label: 'Sources' });
	}

	async function revoke() {
		if (!device) return;
		// The device this page is about no longer exists — the list is the only
		// honest place to be.
		if (await revokeDeviceFlow(device)) backToDevices();
	}
</script>

{#snippet row(label: string, value: string, mono = false)}
	<div class="row">
		<span class="label">{label}</span>
		<span class="value" class:mono>{value}</span>
	</div>
{/snippet}

<Page title={device?.label ?? 'Device'} maxWidth="wide">
	<!--
		Only the way back lives up here. Revoke used to sit beside it — a
		navigation control and an irreversible one about 60px apart — which is
		the same adjacency that made "Start over" wrong at the foot of the old
		Box page. It is now at the bottom, after the facts you would want to read
		before pressing it.
	-->
	{#snippet actions()}
		<Button variant="ghost" onclick={backToDevices}>
			<Icon icon="ri:arrow-left-line" />
			Devices
		</Button>
	{/snippet}

	{#if res.loading}
		<LoadingState />
	{:else if res.error}
		<ErrorState message={res.error} onRetry={res.reload} />
	{:else if !device}
		<EmptyState
			icon="ri:device-line"
			title="No such device"
			message="It may have been revoked. The Devices list has what is still paired."
		/>
	{:else}
		<div class="identity">
			<div class="glyph"><Icon icon={kindIcon(device.kind)} /></div>
			<div>
				<div class="badges">
					<Badge>{kindLabel(device.kind)}</Badge>
					{#if device.is_current}<Badge>This device</Badge>{/if}
				</div>
				<div class="seen">
					Last seen {formatTimeAgo(device.last_seen_at)} · paired {formatTimeAgo(device.paired_at)}
				</div>
			</div>
		</div>

		<section class="chapter">
			<h2>Identity</h2>
			{@render row(
				'Build',
				device.version
					? `${device.version}${device.sha && device.sha !== 'dev' ? ` · ${device.sha}` : ''}${device.channel ? ` · ${device.channel}` : ''}`
					: 'not reported yet',
				!!device.version
			)}
			{@render row('Paired from', device.paired_from_ip ?? '—', true)}
			{@render row('Device id', device.id, true)}
		</section>

		<section class="chapter">
			<h2>Permissions</h2>
			{#if !device.permissions}
				<!-- Null means "never reported", which is the normal state for a
				     device that only views (the Tauri shell, a CLI) and for a
				     collector on a build predating the report. Both are fine, and
				     neither is a denial — a blank list would imply otherwise. -->
				<p class="note">
					This device doesn't report permissions — either it only views the
					record rather than collecting for it, or it runs a build older than
					the report.
				</p>
			{:else}
				{#each granted as perm}
					<div class="perm ok">
						<Icon icon="ri:check-line" />
						<span>{perm.label}</span>
					</div>
				{/each}
				{#each denied as perm}
					<!-- A collector missing a permission isn't an error — nothing
					     crashed, and its other streams are fine. It's a capability
					     the box has been quietly denied, so it reads as a standing
					     warning with the remedy attached. -->
					<div class="perm warn">
						<Icon icon="ri:lock-line" />
						<div>
							<span class="strong">{perm.label} is off</span>
							<span class="muted"> — {perm.costs}.</span>
							{#if canFix && perm.open}
								<div>
									<button class="fix" onclick={() => perm.open?.()}>
										<Icon icon="ri:external-link-line" width="13" />
										Open {perm.label} on this Mac
									</button>
								</div>
							{:else}
								<!-- The honest instruction from another machine. macOS
								     forbids granting these remotely — there is no button
								     we could offer here that would work. -->
								<div class="muted small">
									Granting this needs someone at that machine — macOS has no
									remote path for it.
								</div>
							{/if}
						</div>
					</div>
				{/each}
				{#if granted.length === 0 && denied.length === 0}
					<p class="note">Nothing withheld.</p>
				{/if}
				{#if device.permissions.stale}
					<p class="note">
						This report is stale — the collector may not be running.
					</p>
				{/if}
			{/if}
		</section>

		{#if device.source_id}
			<section class="chapter">
				<h2>Feeds</h2>
				<p class="note">
					What this device sends, and whether it is still arriving, is kept with
					the source it ingests as.
				</p>
				<!-- Not "Open ios in Sources". `source_id` is a catalog key, and
				     printing it raw put a lowercase slug in the middle of a
				     sentence — the page already says which device this is. -->
				<button class="fix" onclick={() => openSource(device.source_id!)}>
					<Icon icon="ri:arrow-right-line" width="13" />
					Open in Sources
				</button>
			</section>
		{/if}

		{#if !device.is_current}
			<!-- The load-bearing sentence on this page. Without it, someone who
			     has seen their own phone's screen reads this one as a degraded
			     version of it and files a bug.
			     Only for OTHER devices: on the device you are holding, the rich
			     panel is one route away (devices/this), so telling you it "can
			     only be seen there" would be false. -->
			<p class="footnote">
				This is everything the box holds about {device.label}. Live detail —
				queue depth, per-stream delivery, battery — is read locally on the
				device itself and never sent here, so it can only be seen there.
			</p>
		{/if}

		<section class="danger">
			<h2>Revoke</h2>
			<p class="note">
				{device.is_current
					? "This is the device you're using. Revoking signs it out immediately; you'll pair again to get back in."
					: `${device.label} loses access to the box right away and stops sending anything new. Nothing it already sent is deleted.`}
			</p>
			<button class="revoke" onclick={revoke}>
				<Icon icon="ri:close-circle-line" width="13" />
				Revoke {device.is_current ? 'this device' : device.label}
			</button>
		</section>
	{/if}
</Page>

<style>
	.danger {
		margin-top: 2.5rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border);
	}

	.revoke {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		margin-top: 6px;
		padding: 5px 10px;
		border: 1px solid color-mix(in srgb, var(--color-error) 45%, transparent);
		border-radius: 6px;
		background: none;
		cursor: pointer;
		font: inherit;
		font-size: 12px;
		color: var(--color-error);
	}

	.revoke:hover {
		background: color-mix(in srgb, var(--color-error) 10%, transparent);
	}

	.revoke:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 1px;
	}

	.identity {
		display: flex;
		align-items: center;
		gap: 14px;
		margin-bottom: 2rem;
	}

	.glyph {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 44px;
		height: 44px;
		flex-shrink: 0;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface-alt);
		color: var(--color-foreground-muted);
		font-size: 20px;
	}

	.badges {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.seen {
		margin-top: 4px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.chapter {
		margin-bottom: 2rem;
	}

	h2 {
		font-size: 14px;
		font-weight: 600;
		margin: 0 0 10px;
	}

	.row {
		display: flex;
		align-items: baseline;
		gap: 16px;
		padding: 6px 0;
		font-size: 13px;
	}

	.label {
		width: 9rem;
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
	}

	.value {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 12px;
	}

	.perm {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		padding: 8px 0;
		font-size: 13px;
	}

	.perm.ok {
		color: var(--color-foreground-muted);
	}

	.perm.warn {
		align-items: flex-start;
		padding: 10px 12px;
		margin: 6px 0;
		border: 1px solid color-mix(in srgb, var(--color-warning) 40%, transparent);
		background: color-mix(in srgb, var(--color-warning) 10%, transparent);
		border-radius: 6px;
	}

	.strong {
		color: var(--color-foreground);
		font-weight: 500;
	}

	.muted {
		color: var(--color-foreground-muted);
	}

	.small {
		font-size: 12px;
		margin-top: 2px;
	}

	.note {
		margin: 0 0 8px;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		max-width: 60ch;
	}

	.fix {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		margin-top: 6px;
		padding: 4px 9px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: none;
		cursor: pointer;
		font: inherit;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.fix:hover {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground);
	}

	.footnote {
		margin: 2.5rem 0 0;
		padding-top: 1.5rem;
		border-top: 1px solid var(--color-border);
		font-size: 12px;
		line-height: 1.6;
		color: var(--color-foreground-subtle);
		max-width: 62ch;
	}
</style>
