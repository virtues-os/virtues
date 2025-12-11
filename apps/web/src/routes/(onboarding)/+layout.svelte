<script lang="ts">
	import "../../app.css";
	import { Toaster } from "svelte-sonner";
	import { onMount } from "svelte";
	import { initTheme } from "$lib/utils/theme";
	import { page } from "$app/stores";

	// Background images
	import libraryImg from "$lib/assets/library.png";
	import greeceImg from "$lib/assets/greece.png";
	import familyImg from "$lib/assets/family.png";
	import paintingImg from "$lib/assets/painting.png";
	import waterfallImg from "$lib/assets/waterfall.png";

	// Components
	import MatrixOverlay from "$lib/components/MatrixOverlay.svelte";

	let { children } = $props();

	// Route-based quotes
	const QUOTES: Record<
		string,
		{ text: string; author: string; source?: string }
	> = {
		"/onboarding/welcome": {
			text: "Know thyself.",
			author: "Delphic maxim",
			source: "Temple of Apollo at Delphi",
		},
		"/onboarding/technology": {
			text: "Technology is neither good nor bad; nor is it neutral.",
			author: "Melvin Kranzberg",
			source: "Kranzberg's First Law",
		},
		"/onboarding/profile": {
			text: "The privilege of a lifetime is to become who you truly are.",
			author: "Carl Jung",
		},
		"/onboarding/places": {
			text: "The world is a book, and those who do not travel read only one page.",
			author: "Augustine of Hippo",
		},
		"/onboarding/tools": {
			text: "We become what we behold. We shape our tools and then our tools shape us.",
			author: "John Culkin",
		},
		"/onboarding/axiology": {
			text: "The unexamined life is not worth living.",
			author: "Socrates",
			source: "Apology, Plato",
		},
	};

	// Step configuration with background images
	const STEPS: Record<
		string,
		{ number: number; total: number; image: string }
	> = {
		"/onboarding/welcome": { number: 1, total: 6, image: libraryImg },
		"/onboarding/technology": { number: 2, total: 6, image: paintingImg },
		"/onboarding/profile": { number: 3, total: 6, image: familyImg },
		"/onboarding/places": { number: 4, total: 6, image: greeceImg },
		"/onboarding/tools": { number: 5, total: 6, image: waterfallImg },
		"/onboarding/axiology": { number: 6, total: 6, image: libraryImg },
	};

	// Convert to Roman numerals
	function toRoman(num: number): string {
		const romans = ["I", "II", "III", "IV", "V", "VI"];
		return romans[num - 1] || num.toString();
	}

	// Reactive values based on current route
	let currentQuote = $derived(
		QUOTES[$page.url.pathname] || QUOTES["/onboarding/welcome"],
	);
	let currentStep = $derived(
		STEPS[$page.url.pathname] || STEPS["/onboarding/welcome"],
	);

	onMount(() => {
		initTheme();
	});
</script>

<Toaster position="top-center" />

<div class="flex min-h-screen overscroll-none">
	<!-- Left: Visual Panel (33%) - Fixed -->
	<aside
		class="w-1/3 fixed top-0 left-0 h-screen bg-[#0a0a0a] overflow-hidden max-md:relative max-md:w-full max-md:h-auto max-md:min-h-[200px]"
	>
		<img
			src={currentStep.image}
			alt=""
			class="absolute inset-0 w-full h-full object-cover object-center z-0"
		/>
		<div
			class="absolute inset-0 bg-linear-to-b from-black/30 to-black/60 z-1"
		></div>
		{#key currentStep.image}
			<MatrixOverlay
				layout="bottom"
				cols={24}
				rows={4}
				cellSize={6}
				cellColor="rgba(255, 255, 255, 0.5)"
			/>
		{/key}
		<div
			class="relative z-2 h-full flex flex-col justify-between p-12 text-white max-md:p-8"
		>
			<span
				class="font-serif text-xl font-normal tracking-wide opacity-90"
				>Virtues</span
			>

			<blockquote class="max-w-xs">
				<p
					class="font-serif text-[22px] font-normal leading-snug tracking-tight mb-4 opacity-95 max-md:text-lg"
				>
					"{currentQuote.text}"
				</p>
				<footer class="flex flex-col gap-0.5">
					<cite
						class="font-sans text-[13px] not-italic font-medium tracking-wide opacity-80"
					>
						{currentQuote.author}
					</cite>
					{#if currentQuote.source}
						<span class="font-sans text-xs opacity-50"
							>{currentQuote.source}</span
						>
					{/if}
				</footer>
			</blockquote>

			<div class="h-5"></div>
		</div>
	</aside>

	<!-- Right: Action Panel (67%) - This is the scroll container -->
	<main
		class="w-2/3 ml-[33.333%] h-screen overflow-y-auto overscroll-none bg-surface flex flex-col max-md:w-full max-md:ml-0 max-md:h-auto max-md:min-h-0"
	>
		<header
			class="font-sans text-xs font-normal tracking-widest uppercase text-foreground-subtle py-6 px-8 text-right max-md:py-4 max-md:px-6"
		>
			Step {toRoman(currentStep.number)} of {toRoman(currentStep.total)}
		</header>

		<div
			class="flex-1 flex items-center justify-center w-full px-12 pb-16 max-md:px-6 max-md:pb-12"
		>
			{@render children()}
		</div>
	</main>
</div>
