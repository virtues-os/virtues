<!--
	"Can't reach your server", on the desktop: the phone's banner
	(MobileShell) for a window with a sidebar. Shown by the app layout while
	`reachability.unreachable`.

	The Mac shows its own copy of the app since 2026-09-29
	(agents/plan/local-ui-plan.md), so the app stays up when the server goes
	away and this bar is where that is said, at launch, mid-session and after
	sleep alike. "Fix it" opens /reconnect, which tells this computer being
	offline from the server being off, refused or moved, and can put a moved
	server back on Wi-Fi over Bluetooth. A browser on the LAN has no radio and
	no pairing to repair, so it gets the words without the button.
-->
<script lang="ts">
	import { goto } from "$app/navigation";

	const ownCopy =
		typeof window !== "undefined" &&
		!!(window as unknown as { __VIRTUES_BACKEND_ORIGIN__?: string }).__VIRTUES_BACKEND_ORIGIN__;
</script>

<div class="bar" role="status">
	<span class="text">Can't reach your server</span>
	{#if ownCopy}
		<button type="button" class="door" onclick={() => void goto("/reconnect")}>Fix it</button>
	{/if}
</div>

<style>
	.bar {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		min-height: 34px;
		margin: 8px 8px 0;
		padding: 4px 4px 4px 12px;
		border: 1px solid var(--color-border);
		border-radius: 9px;
		background: color-mix(in srgb, var(--wash-ink) 4%, transparent);
	}
	.text {
		font-size: 13px;
		color: var(--color-foreground-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.door {
		flex: none;
		min-height: 26px;
		padding: 0 12px;
		border: 0;
		border-radius: 6px;
		background: color-mix(in srgb, var(--wash-ink) 7%, transparent);
		color: var(--color-foreground);
		font-size: 13px;
		font-weight: 550;
		cursor: pointer;
		transition: background-color 0.25s ease-out;
	}
	.door:hover {
		background: color-mix(in srgb, var(--wash-ink) 10%, transparent);
	}
	.door:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
</style>
