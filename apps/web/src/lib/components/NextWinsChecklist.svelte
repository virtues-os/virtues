<!--
  Next wins — the week-one checklist on the empty-chat home.

  Renders the server-derived onboarding steps from /api/setup/state
  (first_source, first_device, remote_access, first_sync). Pure renderer:
  behavior keys off `done`, copy comes from the server `detail`, and `kind`
  is cosmetic only. Shows only after required setup is complete, and hides
  itself once every win is collected (or the user dismisses it).
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import RemoteAccessExplainer from "$lib/components/RemoteAccessExplainer.svelte";
	import { onMount } from "svelte";
	import { setupStateStore } from "$lib/stores/setupState.svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import type { SetupStep } from "$lib/api/client";

	// ── presentation state (server-persisted) ──
	let dismissed = $state(false);
	let collapsed = $state(false);
	// Don't render until prefs arrive — otherwise a dismissed card would
	// flash for a frame on every load before the server says "dismissed".
	let prefsLoaded = $state(false);

	let explainerOpen = $state(false);

	const visible = $derived(
		setupStateStore.loaded &&
			setupStateStore.setupComplete === true &&
			prefsLoaded &&
			!dismissed &&
			!setupStateStore.allDone,
	);

	const total = $derived(setupStateStore.onboarding.length);

	onMount(() => {
		// Fire-and-forget: prefs load in the background and gate rendering.
		loadPrefs();
	});

	async function loadPrefs() {
		try {
			const res = await fetch("/api/assistant-profile");
			if (res.ok) {
				const profile = await res.json();
				const nw = profile.ui_preferences?.nextWins;
				if (nw) {
					dismissed = nw.dismissed ?? false;
					collapsed = nw.collapsed ?? false;
				}
			}
		} catch (error) {
			console.error("Failed to load next-wins state:", error);
		} finally {
			prefsLoaded = true;
		}
	}

	// Persisted server-side (not localStorage) because the box is
	// multi-surface — phone, laptop, kiosk all render this card;
	// localStorage would resurrect it on every new device.
	async function savePrefs() {
		try {
			// First get current preferences, then merge ours in.
			const res = await fetch("/api/assistant-profile");
			if (!res.ok) return;

			const profile = await res.json();
			const existingPrefs = profile.ui_preferences || {};

			const updatedPrefs = {
				...existingPrefs,
				nextWins: { dismissed, collapsed },
			};

			await fetch("/api/assistant-profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ ui_preferences: updatedPrefs }),
			});
		} catch (error) {
			console.error("Failed to save next-wins state:", error);
		}
	}

	function toggleCollapsed() {
		collapsed = !collapsed;
		void savePrefs();
	}

	function dismiss(e: Event) {
		e.stopPropagation();
		dismissed = true;
		void savePrefs();
	}

	// All click-through routing lives in this one function so a future
	// panel/kiosk variant can swap the dispatch without touching markup.
	function dispatchStepAction(step: SetupStep) {
		if (step.done) return;
		switch (step.id) {
			case "first_source":
			case "first_sync":
				spaceStore.openTabFromRoute("/sources", { label: "Sources" });
				break;
			case "first_device":
				spaceStore.openTabFromRoute("/virtues/devices", {
					label: "Devices",
				});
				break;
			case "remote_access":
				explainerOpen = true;
				break;
		}
	}

	function stepIcon(step: SetupStep): string {
		if (step.done) return "ri:checkbox-circle-fill";
		if (step.id === "remote_access") return "ri:error-warning-line";
		return "ri:checkbox-blank-circle-line";
	}

	function stepIconClass(step: SetupStep): string {
		if (step.done) return "text-success";
		if (step.id === "remote_access") return "text-warning";
		return "";
	}
</script>

{#if visible}
	<div class="next-wins">
		<div class="accordion-row">
			<button
				class="accordion-header"
				onclick={toggleCollapsed}
				aria-expanded={!collapsed}
			>
				<span class="chevron" class:rotated={!collapsed}>
					<svg width="12" height="12" viewBox="0 0 12 12">
						<path
							d="M4 2.5L7.5 6L4 9.5"
							stroke="currentColor"
							stroke-width="1.25"
							fill="none"
							stroke-linecap="round"
							stroke-linejoin="round"
						/>
					</svg>
				</span>
				<span class="header-content">
					<span class="header-title">Next wins</span>
					<span class="header-progress"
						>{setupStateStore.doneCount}/{total}</span
					>
				</span>
			</button>
			<button class="dismiss-btn" onclick={dismiss} aria-label="Dismiss">
				<Icon icon="ri:close-line" width="14" />
			</button>
		</div>

		<div class="accordion-content" class:expanded={!collapsed}>
			<div class="accordion-inner">
				<div class="steps-list">
					{#each setupStateStore.onboarding as step (step.id)}
						<button
							class="step-item"
							class:done={step.done}
							onclick={() => dispatchStepAction(step)}
							disabled={step.done}
						>
							<div class="step-indicator">
								<Icon
									icon={stepIcon(step)}
									width="18"
									class={stepIconClass(step)}
								/>
							</div>
							<div class="step-content">
								<div class="step-title">{step.title}</div>
								{#if step.detail}
									<div class="step-detail">{step.detail}</div>
								{/if}
							</div>
						</button>
					{/each}
				</div>
			</div>
		</div>
	</div>

	<RemoteAccessExplainer
		open={explainerOpen}
		onClose={() => (explainerOpen = false)}
		detail={setupStateStore.remoteAccess?.detail}
		kind={setupStateStore.remoteAccess?.kind}
	/>
{/if}

<style>
	/* Premium easing for refined feel */
	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	/* Main container - soft, left-aligned, no card */
	.next-wins {
		width: 100%;
		max-width: 48rem; /* max-w-3xl, matches the chat input */
		margin-top: 0.75rem;
	}

	.accordion-row {
		display: inline-flex;
		align-items: center;
		gap: 2px;
	}

	/* Accordion header - matches ThinkingBlock style */
	.accordion-header {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 8px;
		margin: 0;
		background: transparent;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		color: var(--color-foreground-muted);
		font-size: 13px;
		line-height: 1.5;
		text-align: left;
		transition:
			background-color 0.15s ease,
			color 0.15s ease;
	}

	.accordion-header:hover {
		background-color: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	/* Chevron with rotation animation */
	.chevron {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 12px;
		height: 12px;
		flex-shrink: 0;
		opacity: 0.6;
		transition:
			transform 0.2s cubic-bezier(0.4, 0, 0.2, 1),
			opacity 0.15s ease;
	}

	.chevron.rotated {
		transform: rotate(90deg);
	}

	.accordion-header:hover .chevron {
		opacity: 1;
	}

	.header-content {
		display: flex;
		align-items: baseline;
		gap: 8px;
	}

	.header-title {
		color: var(--color-foreground-muted);
	}

	.header-progress {
		color: var(--color-foreground-muted);
	}

	.header-progress::before {
		content: "·";
		margin-right: 8px;
	}

	.dismiss-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		padding: 0;
		background: transparent;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		color: var(--color-foreground-muted);
		opacity: 0.5;
		transition:
			background-color 0.15s ease,
			color 0.15s ease,
			opacity 0.15s ease;
	}

	.dismiss-btn:hover {
		background-color: var(--color-surface-elevated);
		color: var(--color-foreground);
		opacity: 1;
	}

	/* Accordion content with smooth grid animation */
	.accordion-content {
		display: grid;
		grid-template-rows: 0fr;
		transition: grid-template-rows 250ms var(--ease-premium);
		margin-top: 4px;
		margin-left: 4px; /* Align with header text */
	}

	.accordion-content.expanded {
		grid-template-rows: 1fr;
	}

	.accordion-inner {
		overflow: hidden;
		min-height: 0;
		opacity: 0;
		transform: translateY(-4px);
		transition:
			opacity 200ms ease 50ms,
			transform 200ms ease 50ms;
	}

	.accordion-content.expanded .accordion-inner {
		opacity: 1;
		transform: translateY(0);
	}

	/* Steps list - compact */
	.steps-list {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	/* Step items - minimal, soft */
	.step-item {
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
		width: 100%;
		padding: 0.375rem 0.5rem;
		background: transparent;
		border: none;
		border-radius: 0.375rem;
		cursor: pointer;
		text-align: left;
		transition: all 150ms var(--ease-premium);
	}

	.step-item:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}

	.step-item:active:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.step-item.done {
		cursor: default;
		opacity: 0.6;
	}

	/* Step indicator - icon only, sized like the setup-page rail */
	.step-indicator {
		flex-shrink: 0;
		width: 20px;
		height: 20px;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-foreground-muted);
	}

	/* Step content */
	.step-content {
		flex: 1;
		min-width: 0;
	}

	.step-title {
		font-size: 0.8125rem;
		font-weight: 400;
		color: var(--color-foreground);
		line-height: 1.4;
	}

	.step-item.done .step-title {
		text-decoration: line-through;
		text-decoration-color: var(--color-foreground-muted);
		text-decoration-thickness: 1px;
	}

	.step-detail {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		line-height: 1.4;
	}

	/* Focus states for accessibility */
	.accordion-header:focus-visible,
	.dismiss-btn:focus-visible,
	.step-item:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	/* Reduced motion support */
	@media (prefers-reduced-motion: reduce) {
		.accordion-header,
		.step-item,
		.chevron,
		.accordion-content,
		.accordion-inner {
			transition: none;
		}
	}
</style>
