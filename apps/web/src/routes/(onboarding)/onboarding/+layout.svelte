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
		setCanContinue: (value: boolean) => { canContinue = value; },
		registerStepData: (formData: Record<string, unknown>) => { stepData = formData; },
		initialData: data
	});

	// Navigation configuration with titles
	const STEPS = [
		{ path: "/onboarding/welcome", title: "Welcome" },
		{ path: "/onboarding/technology", title: "Technology" },
		{ path: "/onboarding/profile", title: "Profile" },
		{ path: "/onboarding/places", title: "Places" },
		{ path: "/onboarding/tools", title: "Tools" },
		{ path: "/onboarding/axiology", title: "Values" },
	];

	// Get current step index
	let currentIndex = $derived(STEPS.findIndex(s => s.path === $page.url.pathname));
	let isFirstStep = $derived(currentIndex === 0);
	let isLastStep = $derived(currentIndex === STEPS.length - 1);
	let prevStep = $derived(currentIndex > 0 ? STEPS[currentIndex - 1] : null);
	let nextStep = $derived(currentIndex < STEPS.length - 1 ? STEPS[currentIndex + 1] : null);

	function handleBack() {
		if (isNavigating || !prevStep) return;
		goto(prevStep.path);
	}

	async function saveCurrentStep(): Promise<boolean> {
		const path = $page.url.pathname;

		try {
			// Step 1: Welcome - no data to save (video only)

			// Step 2: Technology - save technology vision, pain points, and features
			if (path === "/onboarding/technology") {
				const res = await fetch("/api/profile", {
					method: "PUT",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({
						technology_vision: stepData.vision,
						pain_point_primary: stepData.painPointPrimary,
						pain_point_secondary: stepData.painPointSecondary || null,
						excited_features: stepData.excitedFeatures
					})
				});
				if (!res.ok) throw new Error("Failed to save technology preferences");
			}

			// Step 3: Profile - save profile and assistant name
			if (path === "/onboarding/profile") {
				// Save user profile
				const profileRes = await fetch("/api/profile", {
					method: "PUT",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({
						preferred_name: stepData.name,
						employer: stepData.occupation,
						theme: stepData.currentTheme
					})
				});
				if (!profileRes.ok) throw new Error("Failed to save profile");

				// Save assistant name
				const assistantRes = await fetch("/api/assistant-profile", {
					method: "PUT",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({ assistant_name: stepData.assistantName })
				});
				if (!assistantRes.ok) throw new Error("Failed to save assistant name");
			}

			// Step 4: Places - save locations via entities API
			if (path === "/onboarding/places") {
				const homePlace = stepData.homePlace as { address: string; latitude?: number; longitude?: number; google_place_id?: string } | null;
				const additionalPlaces = (stepData.additionalPlaces as Array<{ label: string; address: string; latitude?: number; longitude?: number; google_place_id?: string }>) || [];

				// Save home place first (with set_as_home flag)
				if (homePlace && homePlace.latitude != null && homePlace.longitude != null) {
					const homeRes = await fetch("/api/entities/places", {
						method: "POST",
						headers: { "Content-Type": "application/json" },
						body: JSON.stringify({
							label: "Home",
							formatted_address: homePlace.address,
							latitude: homePlace.latitude,
							longitude: homePlace.longitude,
							google_place_id: homePlace.google_place_id,
							set_as_home: true
						})
					});
					if (!homeRes.ok) throw new Error("Failed to save home location");
				}

				// Save additional places
				for (const place of additionalPlaces) {
					if (place.latitude != null && place.longitude != null) {
						const res = await fetch("/api/entities/places", {
							method: "POST",
							headers: { "Content-Type": "application/json" },
							body: JSON.stringify({
								label: place.label,
								formatted_address: place.address,
								latitude: place.latitude,
								longitude: place.longitude,
								google_place_id: place.google_place_id
							})
						});
						if (!res.ok) throw new Error(`Failed to save location: ${place.label}`);
					}
				}
			}

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
			// Last step - complete onboarding
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
			<button
				type="button"
				onclick={handleBack}
				disabled={isNavigating}
				class="px-4 py-2 text-sm font-medium rounded-lg bg-surface-elevated text-foreground hover:bg-accent/10 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
			>
				Back
			</button>
		{:else}
			<div></div>
		{/if}

		<Button variant="primary" onclick={handleContinue} disabled={isNavigating || isSaving || !canContinue}>
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
		from { opacity: 0; }
	}

	@keyframes fade-out {
		to { opacity: 0; }
	}

	@keyframes slide-from-right {
		from { transform: translateX(20px); }
	}

	@keyframes slide-to-left {
		to { transform: translateX(-20px); }
	}

	:global(::view-transition-old(onboarding-content)) {
		animation: 200ms ease-out both fade-out, 200ms ease-out both slide-to-left;
	}

	:global(::view-transition-new(onboarding-content)) {
		animation: 200ms ease-out both fade-in, 200ms ease-out both slide-from-right;
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
