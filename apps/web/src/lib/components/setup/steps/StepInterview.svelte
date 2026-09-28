<!--
	Step 7 — Interview. One question per screen, no transcript.

	The interviewer is the one the getting-started chat already runs: the
	same chat (`chat_getting_started`), the same prompt, the same close
	(`write_it_up` writes the "In your own words" document and the step
	settles on it). What changed is what the person sees. A transcript
	invites scrolling back and reading the whole exchange as a chat; the
	interview is not a chat, it is a sequence of questions, so each screen
	holds the one being asked and a place to answer it.

	CHAPTERS COME FROM THE TIMELINE, ONCE. Setup has a step for drawing them,
	so the interview never teaches them a second time: with none drawn, its
	first screen offers the Timeline (or a plain start without them), and a
	start without them asks for them in one plain line, not the chat room's
	opening essay and example table, which repeated the step before it
	(2026-09-25).

	THE TIMELINE IS THE FIRST ANSWER. The interview opens by asking for your
	chapters, and in Setup you have just drawn them. So when chapters exist,
	beginning sends them as the first reply, in words, and the first screen
	is the interviewer taking them up — never the same question twice.
	Without chapters the authored opening asks for them, as it always has.

	SKIPPING A QUESTION is a reply, "I'd rather skip this one." The prompt
	honors a skip instantly and never remarks on it; there is no
	question-level skip on the server, and inventing one would give the
	interviewer a gap it cannot see.

	THE PART, NOT THE COUNT (2026-09-28). "Question 4" counted toward
	nothing, so the interview felt endless while the intro promised six
	parts. The interviewer now ends each turn with a hidden HTML comment,
	"part: N" (agent/prompt.rs, "The territory"; never quote the comment's
	own delimiters in this block, which is itself one), and the eyebrow reads
	"Part 2 of 6 ·
	What sets you apart". Until it says, the eyebrow is just its name.

	Everything is derived on arrival: the transcript from the instant the
	interview began tells which question is open, so leaving mid-interview
	(Finish later) and coming back lands on the same question.
-->
<script lang="ts">
	import { onDestroy, onMount, tick } from "svelte";
	import { fade, fly } from "svelte/transition";
	import { cubicOut } from "svelte/easing";
	import Icon from "$lib/components/Icon.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import ThinkingMark from "$lib/components/ThinkingMark.svelte";
	import { chatInstances } from "$lib/stores/chatInstances.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { getChat, listLifeChapters, type LifeChapter } from "$lib/api/client";
	import { readNextChapter } from "../nextChapter";
	import { getMe } from "$lib/wiki/api";
	import { GETTING_STARTED_CHAT_ID } from "$lib/components/chat/getting-started/getting-started";
	import { INTERVIEW_OPENING_ASK, INTERVIEW_OPENING_BODY } from "$lib/components/chat/interview/interview";
	import { toUiMessage } from "$lib/components/chat/state/transcript";
	import { isAppleKeyboard } from "$lib/utils/platform";
	import { setup } from "../setup.svelte";
	import StepFrame from "../StepFrame.svelte";

	let {
		onnext,
		onskip,
		ondraw,
	}: {
		onnext: () => void;
		onskip?: () => void;
		/** Back to the Timeline, to draw the chapters the interview starts from. */
		ondraw?: () => void;
	} = $props();

	/** Setup's own ask for chapters when none were drawn: one line. */
	const ASK_CHAPTERS =
		"Let's start with the chapters of your life. What would you call them, roughly in order? Rough names and rough years are enough.";
	/** Drawn chapters are sent for you at most once per visit. */
	let chaptersSent = false;

	type Phase = "loading" | "intro" | "asking" | "waiting" | "closed";
	let phase = $state<Phase>("loading");
	let question = $state("");
	/** The authored opening carries an example table under its ask. */
	let opening = $state(false);
	let answer = $state("");
	let error = $state<string | null>(null);
	let chapters = $state<LifeChapter[]>([]);
	let field = $state<HTMLTextAreaElement | null>(null);
	/** Which of the six parts the interviewer says this question is in.
	 *  It ends each turn with `<!-- part: N -->` (prompt.rs), read here and
	 *  removed before anything is shown. Null until it says. */
	let part = $state<number | null>(null);
	const PARTS = [
		"Your chapters",
		"What sets you apart",
		"Who you admire",
		"What pulls at you",
		"What you believe",
		"A good day",
	];
	const MARKER = /<!--\s*part:\s*(\d)\s*-->\s*$/;
	/** The question without its marker (or the start of one, mid-stream). */
	function unmarked(text: string): string {
		return text.replace(MARKER, "").replace(/<!--[^>]*$/, "").trimEnd();
	}
	function readPart(text: string) {
		const m = text.match(MARKER);
		const n = m ? Number(m[1]) : NaN;
		if (n >= 1 && n <= PARTS.length) part = n;
	}
	const touch = typeof window !== "undefined" && !!window.matchMedia?.("(pointer: coarse)").matches;

	const name = $derived(setup.assistantName);

	/** The opening of "In your own words": its first paragraphs, plain, up to
	 *  about three hundred characters, cut at a sentence. */
	let excerpt = $state<string[] | null>(null);
	async function loadExcerpt() {
		const me = await getMe().catch(() => null);
		const doc = me?.article?.trim();
		if (!doc) return;
		const paras = doc
			.split(/\n\s*\n/)
			.map((p) => p.trim())
			.filter((p) => p && !/^(#|>|[-*] |\d+\. |\|)/.test(p))
			.map((p) => p.replace(/[*_`]/g, "").replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").replace(/\s+/g, " "));
		const out: string[] = [];
		let used = 0;
		for (const p of paras) {
			if (used >= 300) break;
			if (used + p.length <= 360) {
				out.push(p);
				used += p.length;
				continue;
			}
			const cut = p.slice(0, 360 - used).match(/^.*[.!?](?=\s|$)/);
			if (cut) out.push(cut[0]);
			break;
		}
		if (out.length) excerpt = out;
	}
	$effect(() => {
		if (phase === "closed" && excerpt === null) void loadExcerpt();
	});
	const still =
		typeof window !== "undefined" && window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

	const chat = chatInstances.getOrCreate({
		conversationId: GETTING_STARTED_CHAT_ID,
		getModel: () => undefined,
		getProjectId: () => null,
	});
	onDestroy(() => chatInstances.release(GETTING_STARTED_CHAT_ID));

	type Turn = { role: string; text: string; subject?: string; createdAt?: Date; closes: boolean };

	function textOf(m: { parts?: { type: string; text?: string; state?: string; output?: unknown }[] }): string {
		return (m.parts ?? [])
			.filter((p) => p.type === "text" && p.text)
			.map((p) => p.text!.trim())
			.filter(Boolean)
			.join("\n\n");
	}
	function closesIn(m: { parts?: { type: string; state?: string; output?: unknown }[] }): boolean {
		return (m.parts ?? []).some(
			(p) =>
				p.type === "tool-write_it_up" &&
				p.state === "output-available" &&
				!!(p.output as { document_page_id?: string } | undefined)?.document_page_id,
		);
	}

	/** The interview's own turns: from the instant it began, without the
	 *  room's authored `gs:` lines, which share the chat. */
	async function transcript(): Promise<Turn[]> {
		const since = gettingStarted.state?.interview_started_at;
		const from = since ? new Date(since).getTime() : 0;
		const data = await getChat<{ messages?: unknown[] }>(GETTING_STARTED_CHAT_ID);
		return (data.messages ?? [])
			.map((m) => toUiMessage(m, new Map()))
			.filter((m) => m.role === "user" || m.role === "assistant")
			.filter((m) => !m.subject?.startsWith("gs:"))
			.filter((m) => !m.createdAt || m.createdAt.getTime() >= from)
			.map((m) => ({
				role: m.role,
				text: textOf(m as never),
				subject: m.subject,
				createdAt: m.createdAt,
				closes: closesIn(m as never),
			}));
	}

	function interviewDone(): boolean {
		return gettingStarted.step("interview")?.status === "done";
	}

	/** Read where the interview stands and show that screen. */
	async function place() {
		error = null;
		await gettingStarted.refresh();
		if (interviewDone()) {
			phase = "closed";
			return;
		}
		if (!gettingStarted.state?.interview_started_at) {
			phase = "intro";
			return;
		}
		const turns = await transcript();
		if (turns.some((t) => t.closes)) {
			phase = "closed";
			return;
		}
		const last = turns[turns.length - 1];
		if (!last) {
			// Begun, and nothing said yet: drawn chapters are the first answer
			// (again, if the first send never landed); without them, one line.
			if (chapters.length > 0 && !chaptersSent) {
				chaptersSent = true;
				await send(chaptersInWords(chapters));
				return;
			}
			// In Setup with nothing drawn, the first screen again: it offers
			// the Timeline rather than asking for chapters in a text box.
			if (ondraw) phase = "intro";
			else ask(INTERVIEW_OPENING_ASK, true);
			return;
		}
		if (last.role === "assistant" && last.text) {
			ask(last.text, false);
			return;
		}
		// Their reply is the last word: a turn is still running on the
		// server (the box keeps it going when the view leaves), or it failed.
		phase = "waiting";
		try {
			await chat.resumeStream();
		} catch {
			/* nothing live to rejoin */
		}
		const again = await transcript();
		const tail = again[again.length - 1];
		if (tail?.role === "assistant" && tail.text) ask(tail.text, false);
		else {
			error = `${name} didn't answer that one. Send it again, or skip it.`;
			ask(previousQuestion(again), false);
			answer = last.text;
		}
	}

	function previousQuestion(turns: Turn[]): string {
		for (let i = turns.length - 1; i >= 0; i--) {
			if (turns[i].role === "assistant" && turns[i].text) return turns[i].text;
		}
		return INTERVIEW_OPENING_ASK;
	}

	async function ask(text: string, isOpening: boolean) {
		readPart(text);
		question = unmarked(text);
		opening = isOpening;
		phase = "asking";
		await tick();
		field?.focus();
	}

	onMount(() => {
		void (async () => {
			try {
				chapters = await listLifeChapters();
			} catch {
				chapters = [];
			}
			try {
				await place();
			} catch {
				phase = "intro";
				error = "Your server couldn't load the interview. Check your connection and try again.";
			}
		})();
	});

	/** The drawn timeline, as the person would say it. */
	function chaptersInWords(list: LifeChapter[]): string {
		const lines = list.map((c) => {
			const from = c.started_at.slice(0, 4);
			const to = c.ended_at ? c.ended_at.slice(0, 4) : "now";
			return `- ${c.title?.trim() || "A stretch I haven't named"} (${from} to ${to})`;
		});
		const next = readNextChapter();
		const ahead = next ? `\n\nAnd the next chapter, I think: ${next}` : "";
		return `These are my chapters, from the timeline I drew:\n\n${lines.join("\n")}${ahead}`;
	}

	async function begin() {
		error = null;
		phase = "loading";
		try {
			await gettingStarted.startInterview();
		} catch {
			phase = "intro";
			error = "Your server couldn't start the interview. Try again.";
			return;
		}
		if (chapters.length > 0) {
			chaptersSent = true;
			await send(chaptersInWords(chapters));
		} else ask(ondraw ? ASK_CHAPTERS : INTERVIEW_OPENING_ASK, !ondraw);
	}

	/** The reply streaming in, shown as it is written. */
	const streaming = $derived.by(() => {
		if (phase !== "waiting") return "";
		const last = chat.messages[chat.messages.length - 1];
		return last?.role === "assistant" ? unmarked(textOf(last as never)) : "";
	});

	async function send(text: string) {
		const t = text.trim();
		if (!t || phase === "waiting") return;
		error = null;
		phase = "waiting";
		try {
			await chat.sendMessage({ text: t });
		} catch {
			/* read the outcome off the server below */
		}
		answer = "";
		await place();
	}

	function onKey(e: KeyboardEvent) {
		if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			void send(answer);
		}
	}

	// The field grows with the answer rather than scrolling inside itself.
	function grow() {
		if (!field) return;
		field.style.height = "auto";
		field.style.height = `${Math.min(field.scrollHeight, 360)}px`;
	}

	const IN = { y: still ? 0 : 12, duration: still ? 0 : 520, easing: cubicOut, delay: still ? 0 : 120 };
</script>

{#if phase === "loading"}
	<div class="blank" aria-hidden="true"></div>
{:else if phase === "intro"}
	<StepFrame
		title="Tell your story"
		subtitle="The record holds what happened. Only you can say what it meant."
	>
		<p class="facts">Six short parts · Skip any question · Finish later and pick up where you left off</p>
		{#if chapters.length > 0}
			<p class="note centered-note">It starts from the chapters you drew:</p>
			<ol class="chips">
				{#each chapters as c (c.id)}
					<li>
						<span class="chip-title">{c.title?.trim() || "Unnamed"}</span>
						<span class="chip-years">{c.started_at.slice(0, 4)}–{c.ended_at ? c.ended_at.slice(0, 4) : "now"}</span>
					</li>
				{/each}
			</ol>
		{/if}
		{#if chapters.length === 0 && ondraw}
			<p class="note centered-note">
				It starts from the chapters of your life. Draw them first, and the interview picks up from there, or
				<button type="button" class="inline-link" onclick={begin}>begin without them</button>.
			</p>
		{/if}
		{#if error}<p class="err" role="alert">{error}</p>{/if}
		{#snippet actions()}
			{#if chapters.length === 0 && ondraw}
				<button type="button" class="setup-go" onclick={ondraw}>
					Draw your chapters
					<Icon icon="ri:arrow-right-line" width="16" />
				</button>
			{:else}
				<button type="button" class="setup-go" onclick={begin}>
					Begin the interview
					<Icon icon="ri:arrow-right-line" width="16" />
				</button>
			{/if}
			{#if onskip}
				<button type="button" class="setup-past" onclick={onskip}>Skip for now</button>
			{/if}
		{/snippet}
	</StepFrame>
{:else if phase === "closed"}
	<!-- THE PAYOFF IS ON THE PAGE (2026-09-28). The first time someone sees
	     themselves written up is what the whole flow builds to, and this
	     screen used to be a check mark and a sentence about it. It now shows
	     the opening of "In your own words", set like the letter, and the way
	     on reads the rest: Setup's close lands on the document. No separate
	     done icon: the ∴ above is full, which is Setup's one way of saying
	     done. Without a document (the write-up failed), the plain version. -->
	<StepFrame
		title="You've told your story"
		subtitle={excerpt
			? `${name} wrote it up from your answers, in your own words:`
			: `${name} wrote it up from your answers, in your own words. It's in your wiki, and you can change it whenever you like.`}
	>
		{#if excerpt}
			<blockquote class="excerpt" in:fly={IN}>
				{#each excerpt as para, i (i)}<p>{para}</p>{/each}
			</blockquote>
		{/if}
		{#snippet actions()}
			<button type="button" class="setup-go" onclick={onnext}>
				{excerpt ? "Read it all" : "Finish setup"}
				<Icon icon="ri:arrow-right-line" width="16" />
			</button>
		{/snippet}
	</StepFrame>
{:else}
	<section class="sheet" aria-live="polite">
		<p class="who">{part ? `Part ${part} of ${PARTS.length} · ${PARTS[part - 1]}` : name}</p>

		{#if phase === "waiting"}
			<div class="question pending" in:fade={{ duration: still ? 0 : 200 }}>
				{#if streaming}
					<Markdown content={streaming} isStreaming variant="article" />
				{:else}
					<span class="mark"><ThinkingMark depth={3} size={20} /></span>
				{/if}
			</div>
		{:else}
			{#key question}
				<div class="question" in:fly={IN}>
					{#if opening}
						<!-- The example comes first and open: the ask below says
						     "Yours will look nothing like these", so "these" has
						     to be on the page, not folded away. -->
						<div class="example">
							<Markdown content={INTERVIEW_OPENING_BODY} variant="article" />
						</div>
					{/if}
					<Markdown content={question} variant="article" />
				</div>
			{/key}

			<form
				class="answer"
				onsubmit={(e) => {
					e.preventDefault();
					void send(answer);
				}}
			>
				<textarea
					bind:this={field}
					bind:value={answer}
					oninput={() => (grow(), setup.hear())}
					onkeydown={onKey}
					rows="3"
					placeholder="Your answer"
					aria-label="Your answer"
				></textarea>
				{#if error}<p class="err" role="alert">{error}</p>{/if}
				<div class="row">
					<button type="submit" class="setup-go" disabled={!answer.trim()}>
						Answer
						<Icon icon="ri:arrow-right-line" width="16" />
					</button>
					<button type="button" class="setup-past" onclick={() => send("I'd rather skip this one.")}>
						Skip this question
					</button>
					{#if !touch}
						<span class="keys">{isAppleKeyboard ? "⌘↩" : "Ctrl+Enter"} to answer</span>
					{:else}
						<!-- Out loud, for now through the keyboard's own dictation;
						     a recorder of our own would need a transcription route
						     the web app does not have yet (setup-plan.md). -->
						<span class="keys">Tap the microphone on your keyboard to answer out loud</span>
					{/if}
				</div>
			</form>
		{/if}
	</section>
{/if}

<style>
	.blank {
		min-height: 60vh;
	}

	/* THE QUESTION IS THE PAGE, on the center line like every step: the
	   assistant's question set large in the serif, as Names asks "And what
	   should Ari call you?", and the answer written on the paper under it,
	   not typed into a form (2026-09-28: it was the one left-aligned step,
	   a small question over a bordered box). */
	.sheet {
		width: 100%;
		max-width: 40rem;
		margin: 0 auto;
		padding: clamp(2.5rem, 8vh, 5.5rem) 16px 4rem;
		text-align: center;
	}
	/* Three facts, in the step's quietest type: how long, and the two ways
	   out, before anyone starts. */
	.facts {
		margin: -0.5rem 0 0;
		text-align: center;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}
	.note.centered-note {
		max-width: 30rem;
		margin: 1.5rem auto 0;
		text-align: center;
	}
	.keys {
		flex-basis: 100%;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.who {
		margin: 0 0 1.25rem;
		font-size: 12px;
		letter-spacing: 0.02em;
		color: var(--color-foreground-subtle);
	}

	/* The question is the page: the record's serif, at reading size, with
	   the interviewer's lead-in and question kept together as written. */
	.question {
		font-family: var(--font-serif, Georgia, serif);
		font-size: clamp(22px, 2.6vw, 30px);
		line-height: 1.35;
		letter-spacing: -0.005em;
		color: var(--color-foreground);
		text-wrap: balance;
	}
	.question :global(p) {
		margin: 0 0 0.9em;
	}
	.question.pending {
		min-height: 6rem;
		display: grid;
		place-items: center;
		color: var(--color-foreground-muted);
	}
	.mark {
		display: inline-grid;
		color: var(--color-primary);
	}

	.example {
		margin: 0 0 1.5rem;
		text-align: left;
		font-size: 0.95rem;
		color: var(--color-foreground-muted);
	}

	.chips {
		list-style: none;
		margin: 0.9rem 0 0;
		padding: 0;
		display: flex;
		justify-content: center;
		flex-wrap: wrap;
		gap: 8px;
	}
	.chips li {
		display: inline-flex;
		align-items: baseline;
		gap: 8px;
		padding: 6px 12px;
		border-radius: 999px;
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}
	.chip-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 15px;
		color: var(--color-foreground);
	}
	.chip-years {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.answer {
		margin-top: 2rem;
		display: flex;
		flex-direction: column;
		gap: 1.1rem;
	}
	/* Written on the paper: the serif, no box, one hairline that turns
	   primary while you write. Left-aligned inside the column, because an
	   answer runs to paragraphs and centered paragraphs don't read. */
	textarea {
		width: 100%;
		min-height: 5.5rem;
		resize: none;
		padding: 0 0 12px;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		border-radius: 0;
		background: transparent;
		color: var(--color-foreground);
		font-family: var(--font-serif, Georgia, serif);
		font-size: 19px;
		line-height: 1.55;
		text-align: left;
		outline: none;
		transition: border-color var(--m-quick) ease;
	}
	textarea::placeholder {
		color: var(--color-foreground-subtle);
	}
	textarea:focus {
		border-bottom-color: var(--color-primary);
	}
	.row {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-wrap: wrap;
		gap: 1.25rem;
	}

	.inline-link {
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		color: var(--color-primary);
		text-decoration: underline;
		text-underline-offset: 3px;
		cursor: pointer;
	}
	.note {
		margin: 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.err {
		margin: 0.75rem 0 0;
		font-size: 13px;
		color: var(--color-error);
	}

	.excerpt {
		max-width: 32rem;
		margin: 0 auto;
		padding: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 19px;
		line-height: 1.6;
		color: var(--color-foreground);
		text-align: left;
		text-wrap: pretty;
	}
	.excerpt p {
		margin: 0 0 0.9em;
	}
	.excerpt p:last-child {
		margin-bottom: 0;
	}
</style>
