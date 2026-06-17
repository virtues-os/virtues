<!--
  Sidebar "Finish setup" entry — the ONE persistent onboarding surface.

  Replaces the under-chat NextWinsChecklist and the floating BoxSetupNudge (both
  removed): a single quiet, dismissible row that reopens the unified /setup
  onboarding flow. Shows only while onboarding has steps left; dismissal is
  per-session (sessionStorage) so it returns on next launch until setup is
  actually complete — you can't permanently lose the path back.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import { setupStateStore } from "$lib/stores/setupState.svelte";

	let dismissed = $state(
		typeof sessionStorage !== "undefined" &&
			sessionStorage.getItem("setup-entry-dismissed") === "true",
	);

	const remaining = $derived(
		Math.max(0, setupStateStore.onboarding.length - setupStateStore.doneCount),
	);
	const visible = $derived(
		setupStateStore.loaded && !setupStateStore.allDone && remaining > 0 && !dismissed,
	);

	function open() {
		void goto("/setup");
	}
	function dismiss(e: MouseEvent) {
		e.stopPropagation();
		dismissed = true;
		if (typeof sessionStorage !== "undefined")
			sessionStorage.setItem("setup-entry-dismissed", "true");
	}
</script>

{#if visible}
	<div class="setup-row">
		<button class="sidebar-interactive setup-main" onclick={open} title="Finish setting up Virtues">
			<Icon icon="ri:rocket-2-line" width="16" class="sidebar-icon" />
			<span class="sidebar-label">Finish setup</span>
			<span class="setup-count">{remaining}</span>
		</button>
		<button class="setup-dismiss" onclick={dismiss} aria-label="Hide for now" title="Hide for now">
			<Icon icon="ri:close-line" width="14" />
		</button>
	</div>
{/if}

<style>
	@reference "../../../app.css";
	@reference "$lib/styles/sidebar.css";

	.setup-row {
		position: relative;
		display: flex;
		align-items: center;
	}

	.setup-main {
		flex: 1;
		min-width: 0;
	}

	.setup-count {
		margin-left: auto;
		font-size: 11px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
		background: var(--color-surface-alt);
		border-radius: 999px;
		padding: 0 6px;
		line-height: 18px;
	}

	.setup-dismiss {
		position: absolute;
		right: 4px;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		border-radius: 4px;
		color: var(--color-foreground-subtle);
		opacity: 0;
		transition: opacity 120ms ease;
	}

	.setup-row:hover .setup-dismiss {
		opacity: 1;
	}
	.setup-row:hover .setup-count {
		opacity: 0;
	}
	.setup-dismiss:hover {
		background: var(--color-surface-alt);
		color: var(--color-foreground);
	}
</style>
