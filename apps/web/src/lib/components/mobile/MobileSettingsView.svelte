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
	import { search } from "$lib/stores/search.svelte";
	import { pinsStore } from "$lib/stores/pins.svelte";
	import { isEmoji } from "$lib/utils/iconHelpers";
	import MobileDeviceScreen from "./MobileDeviceScreen.svelte";

	interface Row {
		label: string;
		icon: string;
		route: string;
	}

	const pages: Row[] = [
		{ label: "Chats", icon: "ri:chat-1-line", route: "/chat-history" },
		{ label: "Narrative", icon: "ri:quill-pen-line", route: "/narrative-identity" },
		{ label: "Notebooks", icon: "ri:booklet-line", route: "/notebooks" },
		{ label: "Wiki", icon: "ri:book-open-line", route: "/wiki" },
		{ label: "Bookmarks", icon: "ri:bookmark-line", route: "/bookmarks" },
		{ label: "Drive", icon: "ri:cloud-line", route: "/drive" },
		{ label: "Applets", icon: "ri:flashlight-line", route: "/applets" },
	];

	// What the user deliberately put within reach. The desktop keeps these in
	// the sidebar's Desk, which the phone doesn't render — so pinning something
	// on the desktop and then picking up your phone lost it entirely.
	const pinned = $derived(pinsStore.pins);

	function openPin(pin: { url: string; label: string | null }) {
		open(pin.url, pin.label || pin.url);
	}

	function openSearch() {
		mobileLayout.closeMenu();
		search.show();
	}

	// One Settings room, flat sections — mirrors the desktop sidebar's single door.
	const settings: Row[] = [
		{ label: "You", icon: "ri:user-3-line", route: "/virtues/you" },
		{ label: "Assistant", icon: "ri:sparkling-line", route: "/virtues/assistant" },
		{ label: "Sources", icon: "ri:plug-line", route: "/virtues/sources" },
		{ label: "Billing", icon: "ri:bank-card-line", route: "/virtues/billing" },
		{ label: "Box", icon: "ri:computer-line", route: "/virtues/box" },
		{ label: "Devices", icon: "ri:device-line", route: "/virtues/devices" },
		{ label: "Developer", icon: "ri:code-s-slash-line", route: "/virtues/developer" },
	];

	function open(route: string, label: string) {
		windowShellStore.openTabFromRoute(route, { label });
		mobileLayout.closeMenu();
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
				<!-- Search first, and as a field rather than a list row. The phone
				     had no way to find anything at all before this — only to
				     browse — so it should not arrive looking like the seventh
				     item in a directory. Tapping hands off to the same palette
				     ⌘K opens on the desktop. -->
				<button class="search-entry" onclick={openSearch}>
					<Icon icon="ri:search-line" width={18} />
					<span>Search or ask…</span>
				</button>

				{#if pinned.length > 0}
					<div class="group-label">Pinned</div>
					<div class="card">
						{#each pinned as pin (pin.id)}
							<button class="row" onclick={() => openPin(pin)}>
								{#if pin.icon && isEmoji(pin.icon)}
									<span class="pin-emoji">{pin.icon}</span>
								{:else}
									<Icon
										icon={pin.icon || "ri:bookmark-line"}
										width={18}
										style={pin.color ? `color: var(--cat-${pin.color})` : undefined}
									/>
								{/if}
								<span>{pin.label || pin.url}</span>
								<Icon icon="ri:arrow-right-s-line" width={18} />
							</button>
						{/each}
					</div>
				{/if}

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
		bottom: calc(var(--tabbar-reserve) + env(safe-area-inset-bottom));
		z-index: var(--z-sticky);
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
	/* Dressed as the field it stands in for, not as a row. It sits above the
	   groups because finding a thing is a different act from browsing to it. */
	.search-entry {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		margin-top: 8px;
		padding: 11px 12px;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--hover-bg, color-mix(in srgb, var(--color-foreground) 4%, transparent));
		color: var(--color-foreground-muted);
		font-size: 15px;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}

	.search-entry:active {
		opacity: 0.6;
	}

	.pin-emoji {
		font-size: 16px;
		line-height: 1;
		width: 18px;
		text-align: center;
	}

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
