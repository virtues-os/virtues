<script lang="ts">
	import { getContext } from "svelte";
	import { beforeNavigate } from "$app/navigation";
	import { Textarea } from "$lib";

	// Get onboarding context to control continue button and register data
	const { setCanContinue, registerStepData, initialData } = getContext<{
		setCanContinue: (value: boolean) => void;
		registerStepData: (data: Record<string, unknown>) => void;
		initialData: {
			profile?: {
				technology_vision?: string;
				pain_point_primary?: string;
				pain_point_secondary?: string;
				excited_features?: string[];
			};
		};
	}>("onboarding");

	// Track initial vision to detect changes
	const initialVision = initialData?.profile?.technology_vision || "";

	// AutoSave helper
	async function saveProfileField(
		field: string,
		value: string | string[] | null,
	): Promise<void> {
		const response = await fetch("/api/profile", {
			method: "PUT",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({ [field]: value }),
		});
		if (!response.ok) throw new Error(`Failed to save ${field}`);
	}

	// Pain points with categories and underlying needs
	const PAIN_POINTS = [
		{
			key: "chaos",
			text: "My life feels fragmented — too many apps, too much information",
			category: "Chaos",
			need: "Integration & simplicity",
		},
		{
			key: "direction",
			text: "I'm busy but not sure I'm spending time on what matters to me",
			category: "Direction",
			need: "Alignment with values",
		},
		{
			key: "self_knowledge",
			text: "I don't really know what I want or who I'm becoming",
			category: "Self-knowledge",
			need: "Identity & telos clarity",
		},
		{
			key: "character",
			text: "I have goals or habits I want to build but struggle to consistently follow through",
			category: "Character",
			need: "Virtue & habit formation",
		},
		{
			key: "vice",
			text: "I'm struggling with a vice or addiction I want to overcome",
			category: "Vice",
			need: "Accountability & self-mastery",
		},
		{
			key: "tech_disillusionment",
			text: "Current AI tools feel shallow — they don't know me",
			category: "Tech",
			need: "Personalization depth",
		},
		{
			key: "legacy",
			text: "I want to capture and make sense of my life",
			category: "Legacy",
			need: "Autobiography & meaning",
		},
	];

	// Features list
	const FEATURES = [
		{
			key: "autobiography",
			label: "Automatic life logging & autobiography",
		},
		{ key: "praxis", label: "Goals, tasks & habit tracking" },
		{ key: "axiology", label: "Values discovery" },
		{ key: "journaling", label: "Journaling & daily reflection" },
		{ key: "ai_chat", label: "AI assistant for everyday questions" },
		{
			key: "health",
			label: "Health tracking (sleep, heart rate, nutrition)",
		},
	];

	// Form state
	let vision = $state(initialData?.profile?.technology_vision || "");
	let painPointPrimary = $state(
		initialData?.profile?.pain_point_primary || "",
	);
	let painPointSecondary = $state(
		initialData?.profile?.pain_point_secondary || "",
	);
	let excitedFeatures = $state<string[]>(
		initialData?.profile?.excited_features || [],
	);

	// Save vision before navigation (in case blur didn't fire)
	beforeNavigate(async () => {
		if (vision !== initialVision) {
			await saveProfileField("technology_vision", vision || null);
		}
	});

	// Toggle feature selection
	function toggleFeature(key: string) {
		if (excitedFeatures.includes(key)) {
			excitedFeatures = excitedFeatures.filter((f) => f !== key);
		} else {
			excitedFeatures = [...excitedFeatures, key];
		}
		// Save immediately
		saveProfileField("excited_features", excitedFeatures);
	}

	// Handle pain point selection
	function selectPrimary(key: string) {
		if (painPointPrimary === key) {
			painPointPrimary = "";
		} else {
			painPointPrimary = key;
			// If secondary was the same, clear it
			if (painPointSecondary === key) {
				painPointSecondary = "";
			}
		}
		// Save immediately
		saveProfileField("pain_point_primary", painPointPrimary || null);
		if (painPointSecondary === key) {
			saveProfileField("pain_point_secondary", null);
		}
	}

	function selectSecondary(key: string) {
		if (key === painPointPrimary) return; // Can't select same as primary
		if (painPointSecondary === key) {
			painPointSecondary = "";
		} else {
			painPointSecondary = key;
		}
		// Save immediately
		saveProfileField("pain_point_secondary", painPointSecondary || null);
	}

	// Update canContinue and register data whenever fields change
	$effect(() => {
		const isValid =
			!!vision.trim() && !!painPointPrimary && excitedFeatures.length > 0;
		setCanContinue(isValid);
		registerStepData({
			vision,
			painPointPrimary,
			painPointSecondary,
			excitedFeatures,
		});
	});
</script>

<div class="markdown w-full max-w-xl mx-auto">
	<header>
		<h1 class="text-4xl!">You + Technology</h1>
		<p class="text-foreground-subtle mt-2">
			Help us understand your relationship with technology and what you
			hope to get from Virtues.
		</p>
	</header>

	<!-- Section 1: Vision -->
	<section class="mt-8">
		<h2>Your Vision</h2>
		<p class="mb-4">
			What's your vision for how AI/technology should augment human life?
		</p>

		<div
			class="mb-4 p-4 bg-surface-elevated rounded-lg border border-border"
		>
			<p class="text-sm text-foreground-subtle mb-2 font-medium">
				Example response:
			</p>
			<p class="text-sm text-foreground-subtle italic">
				"I've tried every todo app and they all feel soulless. I want an
				AI that knows I value deep work and family time, blocks my
				calendar accordingly, and pushes back when I overcommit — not
				one that just helps me do more faster."
			</p>
		</div>

		<Textarea
			bind:value={vision}
			placeholder="Share your vision..."
			rows={4}
			autoSave
			onSave={(val) => saveProfileField("technology_vision", val || null)}
		/>
	</section>

	<!-- Section 2: Pain Points -->
	<section class="mt-10">
		<h2>What Brought You Here?</h2>
		<p class="mb-4">
			Select your <span class="font-medium">primary</span> reason for seeking
			out Virtues, and optionally a secondary one.
		</p>

		<div class="flex flex-col gap-1">
			{#each PAIN_POINTS as point}
				{@const isPrimary = painPointPrimary === point.key}
				{@const isSecondary = painPointSecondary === point.key}
				{@const isSelected = isPrimary || isSecondary}
				<button
					type="button"
					onclick={() => {
						if (!painPointPrimary || isPrimary) {
							selectPrimary(point.key);
						} else {
							selectSecondary(point.key);
						}
					}}
					class="w-full text-left px-2 py-1.5 rounded transition-all cursor-pointer flex items-center gap-2.5 {isSelected
						? 'bg-primary/10'
						: 'hover:bg-surface-elevated'}"
				>
					<div
						class="w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 {isPrimary
							? 'border-primary bg-primary'
							: isSecondary
								? 'border-foreground-subtle bg-foreground-subtle'
								: 'border-border'}"
					>
						{#if isPrimary}
							<span class="text-[9px] text-surface font-bold"
								>1</span
							>
						{:else if isSecondary}
							<span class="text-[9px] text-surface font-bold"
								>2</span
							>
						{/if}
					</div>
					<span
						class="text-sm {isSelected
							? 'text-foreground font-medium'
							: 'text-foreground-subtle'}"
					>
						{point.text}
					</span>
				</button>
			{/each}
		</div>
	</section>

	<!-- Section 3: Features -->
	<section class="mt-10">
		<h2>What Excites You?</h2>
		<p class="mb-4">
			Which features are you most excited about? Select all that interest
			you.
		</p>

		<div class="flex flex-col gap-1">
			{#each FEATURES as feature}
				{@const isSelected = excitedFeatures.includes(feature.key)}
				<button
					type="button"
					onclick={() => toggleFeature(feature.key)}
					class="w-full text-left px-2 py-1.5 rounded transition-all cursor-pointer flex items-center gap-2.5 {isSelected
						? 'bg-primary/10'
						: 'hover:bg-surface-elevated'}"
				>
					<div
						class="w-4 h-4 rounded-full border-2 flex items-center justify-center shrink-0 {isSelected
							? 'border-primary bg-primary'
							: 'border-border'}"
					>
						{#if isSelected}
							<svg
								class="w-2.5 h-2.5 text-surface"
								fill="none"
								stroke="currentColor"
								viewBox="0 0 24 24"
							>
								<path
									stroke-linecap="round"
									stroke-linejoin="round"
									stroke-width="3"
									d="M5 13l4 4L19 7"
								/>
							</svg>
						{/if}
					</div>
					<span
						class="text-sm {isSelected
							? 'text-foreground font-medium'
							: 'text-foreground-subtle'}"
					>
						{feature.label}
					</span>
				</button>
			{/each}
		</div>
	</section>
</div>
