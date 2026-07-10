<script lang="ts">
	/**
	 * Settings — the bottom bar's 5th tab. A directory of every page/route not
	 * in the bar, plus a native "This device" entry that pushes to the collector
	 * dashboard. The iOS "More" pattern: one tab that reaches everything else.
	 *
	 * NOT a modal sheet — it's a full content-area *view*, exactly like Home /
	 * Today / Pages. It fills the content region (top → just above the bottom
	 * bar), the bottom bar stays visible with Settings lit, and you leave by
	 * tapping another tab. No grabber, no drag-to-dismiss (nothing to fight
	 * iOS's top-edge system gesture). `mobileLayout.menuView` toggles root ↔
	 * device.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import MobileDeviceScreen from "./MobileDeviceScreen.svelte";
	import { goto } from "$app/navigation";

	interface Row {
		label: string;
		icon: string;
		route: string;
	}

	const pages: Row[] = [
		{ label: "Chats", icon: "ri:chat-1-line", route: "/chat-history" },
		{ label: "Narrative", icon: "ri:quill-pen-line", route: "/narrative-identity" },
		{ label: "Notebooks", icon: "ri:booklet-line", route: "/notebooks" },
		{ label: "Wiki", icon: "ri:book-open-line", route: "/entities" },
		{ label: "Drive", icon: "ri:cloud-line", route: "/drive" },
		{ label: "Actions", icon: "ri:flashlight-line", route: "/actions" },
	];

	const settings: Row[] = [
		{ label: "Sources", icon: "ri:plug-line", route: "/sources" },
		{ label: "Tools", icon: "ri:tools-line", route: "/tools" },
		{ label: "Profile", icon: "ri:user-3-line", route: "/virtues/account" },
		{ label: "Devices", icon: "ri:device-line", route: "/virtues/devices" },
		{ label: "Activity", icon: "ri:history-line", route: "/virtues/activity" },
		{ label: "Assistant", icon: "ri:robot-line", route: "/virtues/assistant" },
		{ label: "Billing", icon: "ri:bank-card-line", route: "/virtues/billing" },
		{ label: "AI Provider Key", icon: "ri:key-line", route: "/virtues/byo-key" },
		{ label: "System", icon: "ri:computer-line", route: "/virtues/system" },
	];

	function open(route: string, label: string) {
		windowShellStore.openTabFromRoute(route, { label });
		mobileLayout.closeMenu();
	}

	async function signOut() {
		mobileLayout.closeMenu();
		windowShellStore.closeAllTabs();
		await goto("/pair");
	}
</script>

{#if mobileLayout.menuOpen}
	<section class="panel">
		<header class="head">
			{#if mobileLayout.menuView === "device"}
				<button class="back" aria-label="Back" onclick={() => (mobileLayout.menuView = "root")}>
					<Icon icon="ri:arrow-left-line" width={22} />
				</button>
				<h2>This device</h2>
			{:else}
				<h2>More</h2>
			{/if}
		</header>

		<div class="body">
			{#if mobileLayout.menuView === "device"}
				<MobileDeviceScreen />
			{:else}
				{#if mobileLayout.isNativeShell}
					<button class="device-entry" onclick={() => mobileLayout.openDevice()}>
						<div class="de-icon"><Icon icon="ri:smartphone-line" width={20} /></div>
						<div class="de-body">
							<div class="de-title">This device</div>
							<div class="de-sub">Streams, permissions, storage &amp; logs</div>
						</div>
						<Icon icon="ri:arrow-right-s-line" width={20} />
					</button>
				{/if}

				<div class="group-label">Pages &amp; notebooks</div>
				<div class="card">
					{#each pages as row (row.route)}
						<button class="row" onclick={() => open(row.route, row.label)}>
							<Icon icon={row.icon} width={18} />
							<span>{row.label}</span>
							<Icon icon="ri:arrow-right-s-line" width={18} />
						</button>
					{/each}
				</div>

				<div class="group-label">Settings</div>
				<div class="card">
					{#each settings as row (row.route)}
						<button class="row" onclick={() => open(row.route, row.label)}>
							<Icon icon={row.icon} width={18} />
							<span>{row.label}</span>
							<Icon icon="ri:arrow-right-s-line" width={18} />
						</button>
					{/each}
				</div>

				<button class="signout" onclick={signOut}>
					<Icon icon="ri:logout-box-r-line" width={18} />
					<span>Sign out</span>
				</button>
			{/if}
		</div>
	</section>
{/if}

<style>
	/* A full content-area view (not a modal): fills top → just above the bottom
	   bar, opaque, with the bar (z-50) staying on top and tappable. No scrim, no
	   grabber. Leaving = tapping another tab. */
	.panel {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		bottom: calc(50px + env(safe-area-inset-bottom));
		z-index: 45;
		display: flex;
		flex-direction: column;
		/* Match the content pages (surface) so the iOS status-bar strip above the
		   view doesn't read as a mismatched band. Cards provide the grouping. */
		background: var(--color-surface);
		animation: rise 0.22s cubic-bezier(0.32, 0.72, 0, 1);
	}

	.head {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: max(8px, env(safe-area-inset-top)) 16px 8px;
		flex: none;
	}
	.head h2 {
		margin: 0;
		font-size: 20px;
		font-weight: 650;
		flex: 1;
	}
	.back {
		display: flex;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		padding: 0;
	}

	.body {
		flex: 1;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		padding: 0 16px calc(24px + env(safe-area-inset-bottom));
		color: var(--color-foreground);
	}

	.group-label {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-muted);
		margin: 18px 4px 8px;
	}

	.card {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		overflow: hidden;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		padding: 13px 14px;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-foreground);
		font-size: 15px;
		text-align: left;
		cursor: pointer;
	}
	.row:last-child {
		border-bottom: 0;
	}
	.row span {
		flex: 1;
	}
	.row :global(svg:last-child) {
		color: var(--color-foreground-muted);
	}

	/* "This device" entry — highlighted, sits above the directory. */
	.device-entry {
		display: flex;
		align-items: center;
		gap: 12px;
		width: 100%;
		margin-top: 8px;
		padding: 14px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
	}
	.de-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		border-radius: 9px;
		background: color-mix(in srgb, var(--color-primary, #2b6cff) 16%, transparent);
		color: var(--color-primary, #2b6cff);
	}
	.de-body {
		flex: 1;
	}
	.de-title {
		font-size: 15px;
		font-weight: 600;
	}
	.de-sub {
		font-size: 12px;
		color: var(--color-foreground-muted);
		margin-top: 1px;
	}
	.device-entry :global(svg:last-child) {
		color: var(--color-foreground-muted);
	}

	.signout {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		width: 100%;
		margin: 22px 0 8px;
		padding: 14px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: transparent;
		color: var(--color-danger, #e5484d);
		font-size: 15px;
		font-weight: 550;
		cursor: pointer;
	}

	/* Gentle rise + fade on enter — a view swap, not a modal slide. */
	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
</style>
