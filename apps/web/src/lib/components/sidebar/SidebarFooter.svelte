<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { sidebarMode } from "$lib/stores/sidebarMode.svelte";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// Two doors, both of which swap the sidebar into their own mode rather than
	// navigating anywhere directly — see lib/sidebar/modes.ts. Developer is its
	// own door now instead of a section inside Settings, which is what let
	// Settings drop the second row of underline tabs it had grown.
	//
	// There is no "Sign Out" — auth is the device's proven iroh key, not a
	// server session; to drop this device use Settings → Devices → Unpair.
	const doors = [
		{ id: "developer", label: "Developer", icon: "ri:code-s-slash-line" },
		{ id: "settings", label: "Settings", icon: "ri:settings-4-line" },
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
			<Icon icon={door.icon} width="16" />
			{#if !collapsed}<span>{door.label}</span>{/if}
		</button>
	{/each}
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

	.door.collapsed {
		justify-content: center;
		gap: 0;
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
