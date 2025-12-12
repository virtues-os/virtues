<script lang="ts">
	import { setContext } from "svelte";
	import { toast } from "svelte-sonner";

	import { goto, onNavigate } from "$app/navigation";
	import { page } from "$app/stores";

	import Button from "$lib/components/Button.svelte";

	let { children, data } = $props();

	// Navigation debouncing state
	let isNavigating = $state(false);
	let isSaving = $state(false);

	// Form validation state - child pages can set this via context
	let canContinue = $state(true);

	// Step data - child pages register their form data here
	let stepData = $state<Record<string, unknown>>({});

	setContext("onboarding", {
		setCanContinue: (value: boolean) => {
			canContinue = value;
		},
		registerStepData: (formData: Record<string, unknown>) => {
			stepData = formData;
		},
		initialData: data,
	});

	// Navigation configuration with titles
	const STEPS = [
		{ path: "/onboarding/welcome", title: "Welcome" },
		{ path: "/onboarding/technology", title: "Technology" },
		{ path: "/onboarding/profile", title: "Profile" },
		{ path: "/onboarding/places", title: "Places" },
		{ path: "/onboarding/tools", title: "Tools" },
		// { path: "/onboarding/axiology", title: "Values" },
	];

	// Get current step index
	let currentIndex = $derived(
		STEPS.findIndex((s) => s.path === $page.url.pathname),
	);
	let isFirstStep = $derived(currentIndex === 0);
	let isLastStep = $derived(currentIndex === STEPS.length - 1);
	let prevStep = $derived(currentIndex > 0 ? STEPS[currentIndex - 1] : null);
	let nextStep = $derived(
		currentIndex < STEPS.length - 1 ? STEPS[currentIndex + 1] : null,
	);

	function handleBack() {
		if (isNavigating || !prevStep) return;
		goto(prevStep.path);
	}

	async function saveCurrentStep(): Promise<boolean> {
		const path = $page.url.pathname;

		try {
			// Step 1: Welcome - no data to save (video only)

			// Step 2: Technology - auto-saves via Textarea/button handlers

			// Step 3: Profile - auto-saves via Input components

			// Step 4: Places - auto-saves via place handlers

			return true;
		} catch (error) {
			console.error("[onboarding] Save error:", error);
			toast.error("Failed to save. Please try again.");
			return false;
		}
	}

	async function handleContinue() {
		if (isNavigating || isSaving) return;

		isSaving = true;
		const saved = await saveCurrentStep();
		isSaving = false;

		if (!saved) return;

		if (nextStep) {
			goto(nextStep.path);
		} else {
			// Last step - mark onboarding complete and redirect
			await fetch("/api/profile", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ is_onboarding: false }),
			});
			goto("/");
		}
	}

	// Use View Transitions API for smooth page transitions
	onNavigate((navigation) => {
		if (!document.startViewTransition) return;
		if (isNavigating) return; // Prevent nested transitions

		isNavigating = true;

		return new Promise((resolve) => {
			document.startViewTransition(async () => {
				resolve();
				await navigation.complete;
				isNavigating = false;
			});
		});
	});
</script>

<div class="step-container">
	<div class="step-content">
		{@render children()}
	</div>

	<footer class="step-footer">
		{#if !isFirstStep}
			<Button
				variant="secondary"
				onclick={handleBack}
				disabled={isNavigating}
			>
				Back
			</Button>
		{:else}
			<div></div>
		{/if}

		<Button
			variant="primary"
			onclick={handleContinue}
			disabled={isNavigating || isSaving || !canContinue}
		>
			{isSaving ? "Saving..." : isLastStep ? "Complete" : "Continue"}
		</Button>
	</footer>
</div>

<style>
	.step-container {
		width: 100%;
		display: flex;
		flex-direction: column;
		min-height: 100%;
	}

	.step-content {
		flex: 1;
		view-transition-name: onboarding-content;
	}

	/* View Transition animations */
	@keyframes fade-in {
		from {
			opacity: 0;
		}
	}

	@keyframes fade-out {
		to {
			opacity: 0;
		}
	}

	@keyframes slide-from-right {
		from {
			transform: translateX(20px);
		}
	}

	@keyframes slide-to-left {
		to {
			transform: translateX(-20px);
		}
	}

	:global(::view-transition-old(onboarding-content)) {
		animation:
			200ms ease-out both fade-out,
			200ms ease-out both slide-to-left;
	}

	:global(::view-transition-new(onboarding-content)) {
		animation:
			200ms ease-out both fade-in,
			200ms ease-out both slide-from-right;
	}

	.step-footer {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 16px;
		padding-top: 48px;
		margin-top: auto;
		max-width: 36rem; /* max-w-xl */
		width: 100%;
		margin-left: auto;
		margin-right: auto;
	}
</style>
