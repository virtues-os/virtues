<script lang="ts">
	/**
	 * Settings — the bottom bar's 5th tab. A directory of every page/route not
	 * in the bar, including a promoted "This device" entry. The iOS "More"
	 * pattern: one tab that reaches everything else.
	 *
	 * A DIRECTORY, with no screen of its own. It used to draw one — "This
	 * device" pushed MobileDeviceScreen in here behind a back chevron, toggled
	 * by `mobileLayout.menuView`. That component is a route now
	 * (ThisDeviceView at /virtues/devices/this), reached identically from here
	 * and from the Devices list, so the toggle and its header went with it.
	 *
	 * NOT a modal sheet — it's a full content-area *view*, exactly like Home /
	 * Today / Pages. It fills the content region (top → just above the bottom
	 * bar), the bottom bar stays visible with Settings lit, and you leave by
	 * tapping another tab. No grabber, no drag-to-dismiss (nothing to fight
	 * iOS's top-edge system gesture).
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { search } from "$lib/stores/search.svelte";
	import { pinsStore } from "$lib/stores/pins.svelte";
	import { isEmoji } from "$lib/utils/iconHelpers";

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
		{ label: "System", icon: "ri:computer-line", route: "/virtues/system" },
		{ label: "Network", icon: "ri:wifi-line", route: "/virtues/network" },
		{ label: "Software", icon: "ri:box-3-line", route: "/virtues/software" },
		{ label: "Devices", icon: "ri:device-line", route: "/virtues/devices" },
		{ label: "Display", icon: "ri:tv-2-line", route: "/virtues/display" },
		{ label: "Developer", icon: "ri:code-s-slash-line", route: "/virtues/developer" },
	];

	function open(route: string, label: string) {
		windowShellStore.openTabFromRoute(route, { label });
		mobileLayout.closeMenu();
	}
</script>

{#if mobileLayout.menuOpen}
	<section class="panel">
		<!--
			One header, because this view no longer has a second screen. "This
			device" used to be pushed in here behind a back chevron; it is a route
			now (ThisDeviceView at /virtues/devices/this), which is what lets this
			be what its doc comment always claimed — a directory, with no view of
			its own.
		-->
		<header class="head">
			<h2>More</h2>
		</header>

		<div class="body">
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

			<!--
				Keeps its hero position above both cards: on a phone the device
				IS the sensor, so "is this thing collecting?" is the first
				question, not a settings detail. What changed (2026-08-17) is
				where it goes — `/virtues/devices/this`, the same screen the
				Devices list opens — instead of a view this component drew
				itself. It was the only entry here that wasn't a route push,
				and the cost of that was one panel rendering under two
				different headers with two different back buttons depending on
				which door you came through.
			-->
			{#if mobileLayout.isNativeShell}
				<button
					class="device-entry"
					onclick={() => open("/virtues/devices/this", "This device")}
				>
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
	/* `.back` lived here for the device screen's own header. That screen is a
	   route now and carries its own way back, so the chevron and its rule went
	   with it. */

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
