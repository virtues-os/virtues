<script lang="ts">
	import { onMount } from "svelte";
	import { sidebarMode } from "$lib/stores/sidebarMode.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
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
	// a colophon: the date, and the time. (Day-of-year was here once and read
	// as trivia — "DAY 210" tells you nothing you wanted to know.)
	let stamp = $state("");
	let clock = $state("");

	// Click the clock to swap 24h ⇄ 12h. Persisted, because a clock that
	// forgets which face you chose is worse than one that never offered.
	let hour12 = $state(false);

	const DAYS = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
	const MONTHS = [
		"JAN", "FEB", "MAR", "APR", "MAY", "JUN",
		"JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
	];

	function tick() {
		const now = new Date();
		stamp = `${DAYS[now.getDay()]} ${MONTHS[now.getMonth()]} ${now.getDate()}`;
		const h = now.getHours();
		const mm = String(now.getMinutes()).padStart(2, "0");
		clock = hour12
			? `${((h + 11) % 12) + 1}:${mm} ${h < 12 ? "AM" : "PM"}`
			: `${String(h).padStart(2, "0")}:${mm}`;
	}

	function toggleClock() {
		hour12 = !hour12;
		try {
			localStorage.setItem("virtues:clock12", hour12 ? "1" : "0");
		} catch {
			// Private mode / storage disabled — the toggle still works this session.
		}
		tick();
	}

	onMount(() => {
		try {
			hour12 = localStorage.getItem("virtues:clock12") === "1";
		} catch {
			// Fall through to the 24h default.
		}
		tick();
		const id = setInterval(tick, 30_000);
		return () => clearInterval(id);
	});

	// Three doors, each of which swaps the sidebar into its own mode rather than
	// navigating anywhere directly — see lib/sidebar/modes.ts. Developer is its
	// own door instead of a section inside Settings, which is what let Settings
	// drop the second row of underline tabs it had grown; Sources left Settings
	// for the same reason, having been one row between Assistant and Billing.
	//
	// Ordered by how often you mean it: Sources answers "is my data still
	// arriving", which is a question worth asking far more often than either of
	// the other two.
	//
	// There is no "Sign Out" — auth is the device's proven iroh key, not a
	// server session; to drop this device use Settings → Devices → Unpair.
	// `href` opens the room's front page as well as swapping the rail. Settings
	// and Developer deliberately don't: their first row is a preference screen
	// you may not have come for, and swapping the rail under a pane you were
	// reading is the cheaper move. Sources is the opposite — Overview *is* the
	// answer to why you opened the door ("is my data still arriving"), so making
	// you click twice for it would be the wrong default.
	const doors = [
		{ id: "sources", label: "Sources", icon: "sources", href: "/sources" },
		{ id: "developer", label: "Developer", icon: "developer", href: null },
		{ id: "settings", label: "Settings", icon: "settings", href: null },
	];

	function openDoor(door: (typeof doors)[number]) {
		sidebarMode.enter(door.id);
		if (door.href) {
			windowShellStore.navigate(door.href, { label: door.label });
		}
	}
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
			onclick={() => openDoor(door)}
			title={door.label}
		>
			<AtlasIcon name={door.icon} />
			{#if !collapsed}<span>{door.label}</span>{/if}
		</button>
	{/each}

	{#if !collapsed}
		<div class="console">
			<span>{stamp}</span>
			<button
				type="button"
				class="console-clock"
				onclick={toggleClock}
				title={hour12 ? "Switch to 24-hour" : "Switch to 12-hour"}
				aria-label={`Time ${clock}. Switch to ${hour12 ? "24" : "12"}-hour clock.`}
			>{clock}</button>
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

	/* A button for the affordance, but it must read as the same piece of text
	   the span was — so inherit the console's type and drop every default. */
	.console-clock {
		appearance: none;
		background: none;
		border: 0;
		padding: 0;
		margin: 0;
		font: inherit;
		letter-spacing: inherit;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.console-clock:hover {
		color: var(--color-foreground);
	}

	.console-clock:focus-visible {
		outline: 1px solid var(--color-border);
		outline-offset: 2px;
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
