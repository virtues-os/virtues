<script lang="ts">
	/**
	 * "You" sheet — the mobile home for everything not in the bottom bar.
	 *
	 * Full-height slide-up overlay with three zones:
	 *  1. Device (native shell only): collector toggles + per-stream sync/logs.
	 *     Stubbed here; wired to the reach/location plugins in the location-e2e pass.
	 *  2. Long-tail nav: the sidebar destinations that didn't make the bar.
	 *  3. Settings + Sign out.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { goto } from "$app/navigation";

	interface Row {
		label: string;
		icon: string;
		route: string;
	}

	const nav: Row[] = [
		{ label: "Chats", icon: "ri:chat-1-line", route: "/chat-history" },
		{ label: "Narrative", icon: "ri:quill-pen-line", route: "/narrative-identity" },
		{ label: "Spaces", icon: "ri:box-3-line", route: "/spaces" },
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
		mobileLayout.closeYou();
	}

	async function signOut() {
		mobileLayout.closeYou();
		windowShellStore.closeAllTabs();
		await goto("/pair");
	}
</script>

{#if mobileLayout.youSheetOpen}
	<!-- Scrim -->
	<button class="scrim" aria-label="Close" onclick={() => mobileLayout.closeYou()}></button>

	<section class="sheet" style="padding-bottom: calc(72px + env(safe-area-inset-bottom));">
		<header class="head">
			<h2>You</h2>
			<button class="close" aria-label="Close" onclick={() => mobileLayout.closeYou()}>
				<Icon icon="ri:close-line" width={22} />
			</button>
		</header>

		{#if mobileLayout.isNativeShell}
			<div class="group-label">This device</div>
			<div class="card">
				<div class="device-row">
					<div class="device-l">
						<Icon icon="ri:map-pin-line" width={18} />
						<span>Location</span>
					</div>
					<span class="device-status">wiring…</span>
				</div>
				<div class="device-row muted">
					<div class="device-l">
						<Icon icon="ri:heart-pulse-line" width={18} />
						<span>Health</span>
					</div>
					<span class="device-status">soon</span>
				</div>
				<button class="device-row link" onclick={() => open("/virtues/activity", "Activity")}>
					<div class="device-l">
						<Icon icon="ri:pulse-line" width={18} />
						<span>Stream logs &amp; payloads</span>
					</div>
					<Icon icon="ri:arrow-right-s-line" width={18} />
				</button>
			</div>
		{/if}

		<div class="group-label">Go to</div>
		<div class="card">
			{#each nav as row (row.route)}
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
	</section>
{/if}

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 60;
		border: 0;
		background: rgba(0, 0, 0, 0.4);
		animation: fade 0.15s ease;
	}

	.sheet {
		position: fixed;
		left: 0;
		right: 0;
		bottom: 0;
		top: max(40px, env(safe-area-inset-top));
		z-index: 61;
		display: flex;
		flex-direction: column;
		overflow-y: auto;
		-webkit-overflow-scrolling: touch;
		background: var(--color-surface-elevated, var(--color-surface));
		border-top-left-radius: 16px;
		border-top-right-radius: 16px;
		padding: 8px 16px 0;
		color: var(--color-foreground);
		animation: slide 0.2s cubic-bezier(0.32, 0.72, 0, 1);
	}

	.head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 0 4px;
		position: sticky;
		top: 0;
	}
	.head h2 {
		margin: 0;
		font-size: 20px;
		font-weight: 650;
	}
	.close {
		display: flex;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
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

	.row,
	.device-row {
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
	.row:last-child,
	.device-row:last-child {
		border-bottom: 0;
	}
	.row span {
		flex: 1;
	}
	.row :global(svg:last-child) {
		color: var(--color-foreground-muted);
	}

	.device-l {
		display: flex;
		align-items: center;
		gap: 12px;
		flex: 1;
	}
	.device-status {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.device-row.muted {
		opacity: 0.6;
		cursor: default;
	}
	.device-row.link {
		color: var(--color-foreground);
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

	@keyframes slide {
		from {
			transform: translateY(100%);
		}
		to {
			transform: translateY(0);
		}
	}
	@keyframes fade {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}
</style>
