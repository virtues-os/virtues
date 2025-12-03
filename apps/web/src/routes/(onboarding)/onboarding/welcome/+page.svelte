<script lang="ts">
	import { getContext } from "svelte";

	// Get onboarding context to control continue button and register data
	const { setCanContinue, registerStepData, initialData } = getContext<{
		setCanContinue: (value: boolean) => void;
		registerStepData: (data: Record<string, unknown>) => void;
		initialData: { profile?: { crux?: string } };
	}>("onboarding");

	let crux = $state(initialData?.profile?.crux || "");

	// Update canContinue and register data whenever fields change
	$effect(() => {
		setCanContinue(!!crux.trim());
		registerStepData({ crux });
	});
</script>

<div class="markdown w-full max-w-xl mx-auto">
	<header class="">
		<h1 class="text-4xl!">Welcome to Virtues</h1>
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
			extract your attention and productize your attention, your Personal AI
			will help you author your life. In an age of extraction, this is subsidiarity
			technology designed to empower.
		</p>
		<p class="mt-4">Let's get started!</p>
	</section>
	<section class="mt-8">
		<h2>Introductary Video</h2>
		<p class="mt-4">
			The following video is 5 minutes and runs through the core
			philosophy of Virtues. How to have a successful onboarding
			experience, use your Personal AI, our Roadmap, and more.
		</p>
		<iframe
			class="w-full mt-8 aspect-video border border-border rounded-xl"
			src="https://www.youtube.com/embed/Nn1Np7mrFck?controls=0&rel=0&modestbranding=1"
			title="The Architecture of Agency"
			frameborder="0"
			allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
			allowfullscreen
		></iframe>
	</section>

	<!-- <section class="mb-10">
		<p class="mb-4">
			To illustrate the difference between generic AI and axiological AI,
			consider the following prompt:
		</p>
		<p class="">
			"I have an afternoon free this Saturday. What should I do?"
		</p>
		<div class="grid grid-cols-2 gap-4">
			<div class="p-5 rounded-xl bg-surface-elevated">
				<h5 class="mb-1!">Generic AI</h5>
				<p class="text-sm text-foreground-subtle mb-4!">
					The Polite Stranger
				</p>
				<p class="mb-0!">
					Here are 3 great ways to spend a Saturday:<br /><br />
					1. Visit a local museum or art gallery.<br />
					2. Go for a hike in nature.<br />
					3. Relax and catch up on sleep.<br /><br />
					<em>"Whatever you choose, make sure you enjoy yourself!"</em
					>
				</p>
			</div>
			<div class="p-5 rounded-xl border border-border">
				<h5 class="mb-1!">Personal AI</h5>
				<p class="text-sm text-foreground-subtle mb-4!">
					The Axiological Navigator
				</p>
				<p class="mb-0! text-base">
					"You have a chess tournament in 7 days with your friends.
					This is your last block of free time to study the Sicilian
					Defense without distraction.<br /><br />
					However, you also missed your workout yesterday. If you want
					to stay disciplined, maybe hit the gym for 45 mins first, then
					study?"
				</p>
			</div>
		</div>
	</section> -->

	<!-- Part 1: The Context (Triangulating Position) -->
	<section class="mt-8">
		<h2>Initial Coordinates</h2>

		<p class="mb-4">
			This correspondence is the only data shared with our team to help
			shape the roadmap.We would value your vision: How do you hope this
			technology augments your life? What goals are you striving for? What
			does the future of Personal AI look like to you?
		</p>

		<label class="flex flex-col gap-2">
			<span class="text-sm text-foreground-subtle"
				>Help us create your Personal AI</span
			>
			<textarea
				class="w-full p-3 bg-surface border border-border rounded-lg text-foreground placeholder:text-foreground-subtle focus:outline-none focus:border-foreground transition-colors resize-none"
				bind:value={crux}
				placeholder="e.g., I am 25, isolated by remote work, and value real-world connection. I need a 'Social Architect' to help me build friendships and date intentionally. My vision is a tool that nudges me to leave the house rather than keeping me trapped in an app."
				rows="5"
			></textarea>
		</label>
	</section>
</div>
