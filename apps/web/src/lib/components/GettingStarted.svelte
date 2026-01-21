<script lang="ts">
	import { onMount } from 'svelte';
	import {
		GETTING_STARTED_STEPS,
		type GettingStartedStep
	} from '$lib/config/getting-started';
	import { createSession, listSources } from '$lib/api/client';
	import { windowTabs } from '$lib/stores/windowTabs.svelte';

	interface Props {
		onCreateSession?: (sessionId: string) => void;
		onFocusInput?: (placeholder?: string) => void;
	}

	let { onCreateSession, onFocusInput }: Props = $props();

	// State
	let expanded = $state(true);
	let loading = $state(true);
	let completedSteps = $state<string[]>([]);
	let skippedSteps = $state<string[]>([]);

	// Auto-complete detection state
	let sourcesConnected = $state(false);
	let devicePaired = $state(false);
	let hasChatSessions = $state(false);

	onMount(async () => {
		await Promise.all([loadGettingStartedState(), loadAutoCompleteData()]);
		loading = false;
	});

	async function loadGettingStartedState() {
		try {
			const res = await fetch('/api/assistant-profile');
			if (res.ok) {
				const profile = await res.json();
				const gs = profile.ui_preferences?.gettingStarted;
				if (gs) {
					completedSteps = gs.completedSteps ?? [];
					skippedSteps = gs.skippedSteps ?? [];
				}
			}
		} catch (error) {
			console.error('Failed to load getting started state:', error);
		}
	}

	async function loadAutoCompleteData() {
		try {
			// Check sources
			const sources = await listSources();
			sourcesConnected = sources.some(
				(s: any) => s.auth_type !== 'device' && s.auth_type !== 'none'
			);
			devicePaired = sources.some(
				(s: any) => s.auth_type === 'device' && s.pairing_status === 'active'
			);

			// Check sessions
			const sessionsRes = await fetch('/api/sessions');
			if (sessionsRes.ok) {
				const sessions = await sessionsRes.json();
				hasChatSessions = (sessions.conversations?.length ?? 0) > 0;
			}
		} catch (error) {
			console.error('Failed to load auto-complete data:', error);
		}
	}

	function isStepComplete(step: GettingStartedStep): boolean {
		// Check explicit completion
		if (completedSteps.includes(step.id)) return true;

		// Check auto-complete
		if (step.autoComplete) {
			switch (step.autoComplete.type) {
				case 'hasSourcesConnected':
					return sourcesConnected;
				case 'hasDevicePaired':
					return devicePaired;
				case 'hasChatSessions':
					return hasChatSessions;
			}
		}
		return false;
	}

	function isStepSkipped(step: GettingStartedStep): boolean {
		return skippedSteps.includes(step.id);
	}

	function isStepDone(step: GettingStartedStep): boolean {
		return isStepComplete(step) || isStepSkipped(step);
	}

	async function handleStepClick(step: GettingStartedStep) {
		// Don't act on skipped or completed steps
		if (isStepDone(step)) return;

		switch (step.action.type) {
			case 'createSession': {
				try {
					const response = await createSession(step.action.title, [
						{
							role: 'assistant',
							content: step.action.content,
							timestamp: new Date().toISOString()
						}
					]);
					await markStepComplete(step.id);
					onCreateSession?.(response.id);
				} catch (error) {
					console.error('Failed to create intro session:', error);
				}
				break;
			}
			case 'navigate':
				// Open in tab system instead of simple navigation
				windowTabs.openTabFromRoute(step.action.href, { label: step.title });
				break;
			case 'focusInput':
				onFocusInput?.(step.action.placeholder);
				await markStepComplete(step.id);
				break;
		}
	}

	async function handleSkipStep(e: Event, stepId: string) {
		e.stopPropagation(); // Prevent triggering the step click
		if (!skippedSteps.includes(stepId)) {
			skippedSteps = [...skippedSteps, stepId];
			await saveState();
		}
	}

	async function markStepComplete(stepId: string) {
		if (!completedSteps.includes(stepId)) {
			completedSteps = [...completedSteps, stepId];
			await saveState();
		}
	}

	async function saveState() {
		try {
			// First get current preferences
			const res = await fetch('/api/assistant-profile');
			if (!res.ok) return;

			const profile = await res.json();
			const existingPrefs = profile.ui_preferences || {};

			// Merge with new getting started state
			const updatedPrefs = {
				...existingPrefs,
				gettingStarted: {
					...existingPrefs.gettingStarted,
					completedSteps,
					skippedSteps
				}
			};

			await fetch('/api/assistant-profile', {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ ui_preferences: updatedPrefs })
			});
		} catch (error) {
			console.error('Failed to save getting started state:', error);
		}
	}

	// Calculate progress - count both completed and skipped
	const doneCount = $derived(
		GETTING_STARTED_STEPS.filter((step) => isStepDone(step)).length
	);
	const totalSteps = GETTING_STARTED_STEPS.length;

	// Check if all steps are done (completed or skipped)
	const allDone = $derived(doneCount === totalSteps);
</script>

{#if loading}
	<div class="getting-started-skeleton">
		<div class="skeleton-header"></div>
		<div class="skeleton-steps">
			{#each Array(6) as _, i}
				<div class="skeleton-step" style="animation-delay: {i * 0.08}s"></div>
			{/each}
		</div>
	</div>
{:else if !allDone}
	<div class="getting-started">
		<button
			class="accordion-header"
			onclick={() => (expanded = !expanded)}
			aria-expanded={expanded}
		>
			<span class="chevron" class:rotated={expanded}>
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
				<span class="header-title">Getting started</span>
				<span class="header-progress">{doneCount}/{totalSteps}</span>
			</span>
		</button>

		<div class="accordion-content" class:expanded>
			<div class="accordion-inner">
				<div class="steps-list">
					{#each GETTING_STARTED_STEPS as step, index (step.id)}
						{@const isComplete = isStepComplete(step)}
						{@const isSkipped = isStepSkipped(step)}
						{@const isDone = isComplete || isSkipped}
						<div class="step-row" class:done={isDone}>
							<button
								class="step-item"
								class:completed={isComplete}
								class:skipped={isSkipped}
								onclick={() => handleStepClick(step)}
								disabled={isDone}
							>
								<div class="step-indicator">
									{#if isComplete}
										<iconify-icon icon="ri:check-line" width="14"></iconify-icon>
									{:else if isSkipped}
										<iconify-icon icon="ri:subtract-line" width="14"></iconify-icon>
									{:else}
										<span class="step-number">{index + 1}</span>
									{/if}
								</div>
								<div class="step-content">
									<div class="step-title">{step.title}</div>
									<div class="step-description">{step.description}</div>
								</div>
								<div class="step-icon">
									<iconify-icon icon={step.icon} width="18"></iconify-icon>
								</div>
							</button>
							{#if !isDone}
								<button
									class="skip-button"
									onclick={(e) => handleSkipStep(e, step.id)}
									title="Skip this step"
								>
									Skip
								</button>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		</div>
	</div>
{/if}

<style>
	/* Premium easing for refined feel */
	:root {
		--ease-premium: cubic-bezier(0.2, 0, 0, 1);
	}

	/* Main container - soft, left-aligned, no card */
	.getting-started {
		width: 100%;
		max-width: 48rem; /* max-w-3xl */
		margin-top: 0.75rem;
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
		content: '·';
		margin-right: 8px;
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

	/* Step row container for step + skip button */
	.step-row {
		display: flex;
		align-items: center;
		gap: 0.375rem;
	}

	.step-row.done {
		opacity: 0.5;
	}

	/* Step items - minimal, soft */
	.step-item {
		display: flex;
		align-items: center;
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

	.step-item:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}

	.step-item:active {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.step-item.completed,
	.step-item.skipped {
		cursor: default;
	}

	.step-item.skipped .step-indicator {
		color: var(--color-foreground-muted);
		opacity: 0.5;
	}

	.step-item.skipped .step-title {
		text-decoration: line-through;
		text-decoration-color: var(--color-foreground-muted);
		text-decoration-thickness: 1px;
	}

	/* Step indicator - minimal circle */
	.step-indicator {
		flex-shrink: 0;
		width: 20px;
		height: 20px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		background: transparent;
		border: 1.5px solid var(--color-border);
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
		transition: all 200ms var(--ease-premium);
	}

	.step-item.completed .step-indicator {
		background: var(--color-primary);
		border-color: var(--color-primary);
		color: white;
	}

	.step-number {
		font-size: 0.625rem;
		font-weight: 500;
	}

	/* Step content - single line */
	.step-content {
		flex: 1;
		min-width: 0;
	}

	.step-title {
		font-size: 0.8125rem;
		font-weight: 400;
		color: var(--color-foreground);
		line-height: 1.3;
	}

	.step-item.completed .step-title {
		text-decoration: line-through;
		text-decoration-color: var(--color-foreground-muted);
		text-decoration-thickness: 1px;
	}

	.step-description {
		display: none; /* Hide descriptions for compact look */
	}

	/* Step icon - subtle */
	.step-icon {
		flex-shrink: 0;
		color: var(--color-foreground-muted);
		opacity: 0.35;
		transition: all 200ms var(--ease-premium);
	}

	.step-item:hover .step-icon {
		opacity: 0.5;
	}

	.step-item.completed .step-icon,
	.step-item.skipped .step-icon {
		opacity: 0.2;
	}

	/* Skip button - minimal text link style */
	.skip-button {
		flex-shrink: 0;
		padding: 0.25rem 0.375rem;
		background: transparent;
		border: none;
		font-size: 0.6875rem;
		font-weight: 400;
		color: var(--color-foreground-muted);
		opacity: 0.5;
		cursor: pointer;
		transition: all 150ms var(--ease-premium);
	}

	.skip-button:hover {
		opacity: 0.8;
	}

	.skip-button:active {
		opacity: 1;
	}

	/* Loading skeleton - minimal */
	.getting-started-skeleton {
		width: 100%;
		max-width: 48rem;
		margin-top: 1rem;
		padding: 0.5rem 0;
	}

	.skeleton-header {
		height: 24px;
		width: 120px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
		border-radius: 0.25rem;
		animation: pulse 1.5s ease-in-out infinite;
	}

	.skeleton-steps {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		padding-top: 0.25rem;
	}

	.skeleton-step {
		height: 32px;
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
		border-radius: 0.375rem;
		animation: pulse 1.5s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}

	/* Focus states for accessibility */
	.accordion-header:focus-visible,
	.step-item:focus-visible,
	.skip-button:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	/* Reduced motion support */
	@media (prefers-reduced-motion: reduce) {
		.accordion-header,
		.step-item,
		.chevron,
		.accordion-content,
		.accordion-inner,
		.step-indicator,
		.skip-button {
			transition: none;
		}

		.skeleton-header,
		.skeleton-step {
			animation: none;
			background: var(--color-surface-elevated);
		}
	}
</style>
