<script lang="ts">
	import SidebarNavItem from "./SidebarNavItem.svelte";
	import type { SidebarNavItemData } from "./types";

	interface Props {
		collapsed?: boolean;
		animationDelay?: number;
	}

	let {
		collapsed = false,
		animationDelay = 0,
	}: Props = $props();

	// One Settings door. Everything that used to sprawl across the footer folder
	// (Sources, Tools, Profile, Devices, System, Developers) now lives inside the
	// Settings room as sections. There is no "Sign Out" — auth is the device's
	// proven iroh key, not a server session; to drop this device use
	// Settings → Box → Devices → Unpair. `pagespace: "virtues"` lights the item
	// for any /virtues/* section.
	const settingsItem: SidebarNavItemData = {
		id: "settings",
		type: "link",
		label: "Settings",
		icon: "ri:settings-4-line",
		href: "/virtues/you",
		pagespace: "virtues",
	};
</script>

<div
	class="footer"
	class:collapsed
	style="animation-delay: {animationDelay}ms; --stagger-delay: {animationDelay}ms"
>
	<SidebarNavItem item={settingsItem} indent={0} {collapsed} isSystemItem={true} />
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
