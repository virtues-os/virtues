<script lang="ts">
	import { getContext } from "svelte";

	// Get onboarding context to control continue button
	const { setCanContinue } = getContext<{
		setCanContinue: (value: boolean) => void;
	}>("onboarding");

	// Welcome page always allows continuing (just video + intro)
	$effect(() => {
		setCanContinue(true);
	});
</script>

<div class="markdown w-full max-w-xl mx-auto">
	<header class="">
		<h1 class="text-4xl! font-serif">Welcome to Virtues</h1>
		<span
			class="inline-block mt-2 mb-4 px-3 py-1.5 font-mono text-[10px] font-medium tracking-widest uppercase text-foreground-subtle border border-border rounded-full"
		>
			Private Beta 2025.4
		</span>
	</header>
	<section class="mt-8">
		<h2>Preface</h2>
		<p class="mt-4">
			This is <span class="text-primary font-medium">Personal AI</span> that
			knows your facts and values so you can live your story more purposefully.
			Because this requires deep intimacy, your data is fully sovereign—single-tenant
			and never shared, not even with us. While other technology platforms
			extract and productize your attention, your Personal AI will help you
			author your life. In an age of extraction, this is subsidiarity technology
			designed to empower.
		</p>
		<p class="mt-4">Let's get started!</p>
	</section>
	<section class="mt-8">
		<h2>Introductory Video</h2>
		<p class="mt-4">
			The following video is 5 minutes and runs through the core
			philosophy of Virtues. How to have a successful onboarding
			experience, use your Personal AI, our Roadmap, and more.
		</p>
		<div
			class="w-full mt-8 aspect-video border border-border rounded-xl bg-stone-100 flex items-center justify-center"
		>
			<span class="text-foreground-subtle"
				>Introductory Video (Coming Soon) 😃
			</span>
		</div>
	</section>
</div>
