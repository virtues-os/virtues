<!--
  "In your own words" — twelve questions, one at a time.

  ONE QUESTION PER SCREEN, not a long form. A page of twelve prompts about
  grief, vice and faith is a wall; it invites skimming, and skimming is how you
  get twelve shallow answers instead of four real ones. One at a time also lets
  each question carry its own guidance, which matters because they are not
  alike — a word target belongs on "list every hobby you've had" and is
  grotesque on "who did you lose".

  THE STOPLIGHT NEVER GOES RED. It fills toward a soft target and stops. There
  is no failing state, no blocking, no minimum. A red light on a question about
  a dead parent would be the detail that defines this product for the person it
  happened to, and no amount of completion data is worth that.

  AUTOSAVE, VISIBLY. An hour of writing is at stake, so the save is debounced
  to a couple of seconds, runs on every pause, and SAYS so. A failure is shown
  in words rather than swallowed — someone who has just written about their
  father deserves to know immediately if it did not land.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { ONBOARDING_QUESTIONS as QUESTIONS, wordCount, type Question } from "./questions";
	import { getInterviewAnswers, saveInterviewAnswer } from "$lib/api/client";
	import { onMount, onDestroy } from "svelte";

	// Shell and progress strip belong to the route; this is only the leaf.
	let { onfinish }: { onfinish: () => void } = $props();

	let idx = $state(0);
	let answers = $state<Record<string, string>>({});
	let completed = $state<Record<string, boolean>>({});
	let loading = $state(true);
	let saveState = $state<"idle" | "saving" | "saved" | "error">("idle");
	let saveError = $state<string | null>(null);
	let timer: ReturnType<typeof setTimeout> | null = null;

	const q = $derived<Question>(QUESTIONS[idx]);
	const text = $derived(answers[q.id] ?? "");
	const words = $derived(wordCount(text));
	// Toward the target and no further. Nothing here reports a shortfall.
	const progress = $derived(q.target ? Math.min(1, words / q.target) : 0);
	const answeredCount = $derived(Object.values(completed).filter(Boolean).length);

	onMount(async () => {
		try {
			for (const a of await getInterviewAnswers()) {
				answers[a.question_id] = a.answer;
				if (a.completed_at) completed[a.question_id] = true;
			}
			// Resume where they stopped rather than at the top — returning to
			// question one after writing eight is its own small insult.
			const next = QUESTIONS.findIndex((x) => !completed[x.id]);
			idx = next === -1 ? 0 : next;
		} catch {
			// A failed LOAD is survivable: they can still write. A failed SAVE is
			// not, and is reported loudly below.
		}
		loading = false;
	});

	onDestroy(() => {
		if (timer) clearTimeout(timer);
	});

	async function persist(markComplete = false) {
		saveState = "saving";
		try {
			await saveInterviewAnswer(q.id, answers[q.id] ?? "", markComplete);
			saveState = "saved";
			saveError = null;
		} catch (e) {
			saveState = "error";
			saveError = e instanceof Error ? e.message : String(e);
		}
	}

	function onInput() {
		if (timer) clearTimeout(timer);
		saveState = "saving";
		timer = setTimeout(() => void persist(), 1500);
	}

	async function go(to: number) {
		if (timer) clearTimeout(timer);
		// Anything written counts as answered. The person decides what "enough"
		// means, not a word count.
		const real = wordCount(answers[q.id] ?? "") > 0;
		await persist(real);
		if (real) completed[q.id] = true;
		idx = Math.max(0, Math.min(QUESTIONS.length - 1, to));
	}
</script>

{#if loading}
	<div><p class="quiet">Finding what you've written…</p></div>
{:else}
	<div>
		<div>
			<header>
				<span class="facet">{q.facet}</span>
				<span class="count">{idx + 1} of {QUESTIONS.length}</span>
			</header>

			<h1 class="ob-h1">{q.prompt}</h1>
			<p class="purpose">{q.purpose}</p>

			<!-- Collapsed by default, and worth the room when opened. These
			     questions ask for grief, vice and faith; whether someone answers
			     properly or skips usually turns on believing the asking is
			     principled. Explaining the MECHANISM earns more than reassurance
			     does — and hiding it keeps the page from lecturing anyone who
			     already wants to write. -->
			<details class="why">
				<summary>Why this question</summary>
				<p>{q.why}</p>
			</details>

			<textarea
				bind:value={answers[q.id]}
				oninput={onInput}
				placeholder="Take your time."
				spellcheck="true"
			></textarea>

			<div class="under">
				{#if q.hint}<p class="hint">{q.hint}</p>{/if}

				<div class="meter">
					{#if q.target}
						<!-- Fills and stops. There is no red. -->
						<div class="bar" aria-hidden="true">
							<div class="fill" style="width: {progress * 100}%"></div>
						</div>
						<span class="words">{words} words</span>
					{:else}
						<span class="words nolimit">as much or as little as you like</span>
					{/if}

					<span class="save" class:err={saveState === "error"}>
						{#if saveState === "saving"}Saving…
						{:else if saveState === "saved"}Saved
						{:else if saveState === "error"}Not saved — {saveError}
						{/if}
					</span>
				</div>
			</div>

			<nav>
				<button class="ob-ghost" onclick={() => go(idx - 1)} disabled={idx === 0}>
					<Icon icon="ri:arrow-left-line" width="15" /> Back
				</button>

				{#if idx < QUESTIONS.length - 1}
					<button class="ob-btn" onclick={() => go(idx + 1)}>
						{words > 0 ? "Next" : "Skip for now"}
						<Icon icon="ri:arrow-right-line" width="15" />
					</button>
				{:else}
					<button class="ob-btn" onclick={async () => { await go(idx); onfinish(); }}>
						Done — write it up
						<Icon icon="ri:quill-pen-line" width="15" />
					</button>
				{/if}
			</nav>

			<!-- Leaving is not abandoning. Everything is already on the box, and
			     saying so is what makes an hour-long document approachable. -->
			<p class="later">
				{answeredCount} of {QUESTIONS.length} answered · everything you write is saved as you
				go, so you can stop here and come back.
			</p>
		</div>
	</div>
{/if}

<style>
	/* The shell, type scale and buttons come from onboarding.css. What follows
	   is only this screen's: the question header, the disclosure, the writing
	   surface and the meter. */
	header {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 11px;
		letter-spacing: 0.16em;
		color: var(--color-foreground-subtle);
	}

	/* A question is a whole sentence, often a long one, so it is set a step
	   below the shared display size — the same face and colour, less shout. */
	h1 {
		font-size: clamp(1.5rem, 3vw, 2rem);
		line-height: 1.15;
		margin: 0.9rem 0 0;
	}

	.purpose {
		margin: 0.85rem 0 0;
		font-size: 14px;
		line-height: 1.6;
		color: var(--color-foreground-muted);
		max-width: 34rem;
	}

	.why {
		margin-top: 1rem;
		font-size: 13px;
	}

	.why summary {
		cursor: pointer;
		color: var(--color-foreground-subtle);
		list-style: none;
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		user-select: none;
	}

	.why summary::-webkit-details-marker {
		display: none;
	}

	.why summary::before {
		content: '＋';
		font-size: 11px;
		opacity: 0.7;
	}

	.why[open] summary::before {
		content: '－';
	}

	.why summary:hover {
		color: var(--color-foreground-muted);
	}

	.why p {
		margin: 0.7rem 0 0;
		padding-left: 0.9rem;
		border-left: 1px solid var(--color-border);
		line-height: 1.65;
		color: var(--color-foreground-muted);
		max-width: 34rem;
	}

	textarea {
		margin-top: 1.5rem;
		width: 100%;
		min-height: 15rem;
		resize: vertical;
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
		border: 1px solid var(--color-border);
		border-radius: 10px;
		padding: 1rem 1.1rem;
		font: inherit;
		font-size: 1rem;
		line-height: 1.65;
		color: var(--color-foreground);
	}

	textarea:focus {
		outline: none;
		border-color: color-mix(in srgb, var(--color-primary) 60%, transparent);
	}

	.under {
		margin-top: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	.hint {
		margin: 0;
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}

	.meter {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.bar {
		width: 7rem;
		height: 3px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
		overflow: hidden;
	}

	/* One colour. A meter that changes colour is a meter that can disapprove. */
	.fill {
		height: 100%;
		background: var(--color-primary);
		transition: width 0.35s ease;
	}

	.words {
		font-variant-numeric: tabular-nums;
	}

	.nolimit {
		font-style: italic;
	}

	.save {
		margin-left: auto;
	}

	.save.err {
		color: #ff9ea1;
	}

	nav {
		margin-top: 1.75rem;
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	/* Both controls come from onboarding.css; the only local adjustment is that
	   the forward button sits in a nav bar rather than under a paragraph, so it
	   does not carry the shared top margin. */
	nav .ob-btn {
		margin-top: 0;
	}

	.later {
		margin: 2rem 0 0;
		font-size: 12.5px;
		color: var(--color-foreground-subtle);
	}

	.quiet {
		color: var(--color-foreground-subtle);
		font-size: 14px;
	}
</style>
