<script lang="ts">
	import { onMount } from "svelte";
	import { sidebarMode } from "$lib/stores/sidebarMode.svelte";
	import AtlasIcon from "./AtlasIcon.svelte";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// The console line: the rail's only facts, set in mono on a hairline.
	//
	// Not a progress bar — a bar dramatizes a number nobody asked for. This is
	// a colophon: which edition of your life this is, and the time. Virtues
	// writes one edition per day, so the day-of-year is the shell's one piece
	// of information no other product could display.
	let stamp = $state("");
	let clock = $state("");

	const DAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
	const MONTHS = [
		"JAN", "FEB", "MAR", "APR", "MAY", "JUN",
		"JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
	];

	function tick() {
		const now = new Date();
		const doy = Math.floor(
			(now.getTime() - new Date(now.getFullYear(), 0, 0).getTime()) / 86400000,
		);
		stamp = `${DAYS[now.getDay()]} ${MONTHS[now.getMonth()]} ${now.getDate()} · DAY ${doy}`;
		clock = `${String(now.getHours()).padStart(2, "0")}:${String(now.getMinutes()).padStart(2, "0")}`;
	}

	onMount(() => {
		tick();
		const id = setInterval(tick, 30_000);
		return () => clearInterval(id);
	});

	// Two doors, both of which swap the sidebar into their own mode rather than
	// navigating anywhere directly — see lib/sidebar/modes.ts. Developer is its
	// own door now instead of a section inside Settings, which is what let
	// Settings drop the second row of underline tabs it had grown.
	//
	// There is no "Sign Out" — auth is the device's proven iroh key, not a
	// server session; to drop this device use Settings → Devices → Unpair.
	const doors = [
		{ id: "developer", label: "Developer", icon: "developer" },
		{ id: "settings", label: "Settings", icon: "settings" },
	];
</script>

<div
	class="footer"
	class:collapsed
	style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
>
	{#each doors as door (door.id)}
		<button
			type="button"
			class="door"
			class:collapsed
			class:active={sidebarMode.activeId === door.id}
			onclick={() => sidebarMode.enter(door.id)}
			title={door.label}
		>
			<AtlasIcon name={door.icon} />
			{#if !collapsed}<span>{door.label}</span>{/if}
		</button>
	{/each}

	{#if !collapsed}
		<div class="console">
			<span>{stamp}</span>
			<span class="console-clock">{clock}</span>
		</div>
	{/if}
</div>

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.footer {
		@apply flex flex-col gap-1 py-3 mt-auto;
		padding-left: 8px;
		/* Staggered load animation (initial mount) */
		animation: sidebar-fade-slide-in 200ms var(--sidebar-transition-easing) backwards;
		/* Staggered expand transition - uses --stagger-delay CSS var */
		opacity: 1;
		transform: translateX(0);
		transition:
			opacity 200ms var(--sidebar-transition-easing) var(--stagger-delay, 0ms),
			transform 200ms var(--sidebar-transition-easing) var(--stagger-delay, 0ms);
	}

	.door {
		display: flex;
		align-items: center;
		gap: var(--sidebar-interactive-gap);
		width: 100%;
		height: var(--sidebar-interactive-height);
		padding: var(--sidebar-interactive-padding);
		border: none;
		border-radius: var(--sidebar-interactive-radius);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: var(--sidebar-interactive-font-size);
		color: var(--sidebar-interactive-color);
	}

	.door :global(.atlas-icon) {
		color: var(--color-foreground-muted);
		opacity: var(--sidebar-icon-opacity);
	}

	.door:hover :global(.atlas-icon) {
		opacity: 1;
	}

	.door.collapsed {
		justify-content: center;
		gap: 0;
	}

	/* The library card + the colophon. Doors read exactly like Library rows —
	   separation is distance, not a second type register. */
	.console {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 8px;
		margin: 6px 0 0;
		padding: 8px 10px 0 var(--sidebar-padding-left-base);
		border-top: 1px solid var(--color-border-subtle);
		font-family: var(--font-mono);
		font-size: 9.5px;
		letter-spacing: 0.07em;
		color: var(--color-foreground-disabled);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		user-select: none;
	}

	.console-clock {
		color: var(--color-foreground-subtle);
	}

	.door:hover {
		background: var(--sidebar-hover-bg);
	}

	.door.active {
		background: var(--sidebar-active-bg);
	}

	.door:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}

	.footer.collapsed {
		@apply items-center;
		padding-left: 4px;
		padding-right: 4px;
		opacity: 0;
		transition:
			opacity var(--sidebar-transition-duration) var(--sidebar-transition-easing),
			transform var(--sidebar-transition-duration) var(--sidebar-transition-easing);
	}
</style>
