<!--
  "In your own words" — the narrative interview, as one conversation.

  THE WHOLE DESIGN IS THREE ARTIFACTS: the door copy, the authored opening,
  and the interviewer's conduct (the system prompt, agent_mode "interview" —
  see agent/prompt.rs INTERVIEW_PROMPT and docs/lsi-plan.md for the design
  history, including everything this replaced: textareas, chapter cards, and
  a five-movement "sitting" that died of its own machinery).

  ONE CHAT, ONE FIXED ID. The conversation IS the material: no per-question
  answers, no filing tools, nothing in between. When the person is done, the
  drafter reads this chat's transcript and arranges THEIR words into the
  document (narrative_draft::INTERVIEW_CHAT_ID — keep the constant in sync).

  NOT DRESSED AS A CHAT APP. No bubbles, no avatars, no streaming shimmer
  theater: the interviewer's turns carry the ∴ mark, the person's turns are
  unmarked serif prose, and the page reads as a manuscript growing downward —
  which is what it is. The input is a writing surface ("Take your time."),
  not a message bar.

  THE OPENING IS AUTHORED, not generated: shown statically before the first
  exchange, never persisted, zero-cost, instant. The system prompt knows it
  was delivered and picks up from the reply (the NEW_USER_PROMPT pattern).

  TODO(door, assets pending): the founder video (poster frame, never
  autoplay) and the founder's own NI document excerpt ("I sat for this
  myself — this is mine") slot between the terms and the territories. The
  door works without them; reciprocity arrives when the assets do.
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { chatInstances } from "$lib/stores/chatInstances.svelte";
	import { getDefaultModel, getInitializationPromise } from "$lib/stores/models.svelte";
	import type { Chat } from "@ai-sdk/svelte";

	/** Mirrors narrative_draft::INTERVIEW_CHAT_ID. */
	const CHAT_ID = "chat_narrative_interview";

	/** The authored opening — the interviewer's first words, delivered free. */
	const OPENING =
		"Your box can keep your days from here on — but everything before it, and " +
		"everything underneath it, only you can tell. I'd like to hear it plainly, " +
		"the way you'd tell a friend. I won't interpret you, I won't press on " +
		"anything you don't offer, and only your words end up in the document. " +
		"Let's start at the beginning: where does your story start, and what were " +
		"its chapters?";

	let { onfinish }: { onfinish: () => void } = $props();

	let began = $state(false);
	let loading = $state(true);
	let draft = $state("");
	let chat = $state<Chat | null>(null);

	const busy = $derived(chat?.status === "submitted" || chat?.status === "streaming");
	const spoke = $derived((chat?.messages ?? []).some((m) => m.role === "user"));

	/** The text of a message — this room renders text parts and nothing else. */
	function textOf(m: { parts?: unknown }): string {
		const parts = (m.parts ?? []) as Array<{ type?: string; text?: string }>;
		return parts
			.filter((p) => p.type === "text" && p.text)
			.map((p) => p.text)
			.join("");
	}

	onMount(async () => {
		await getInitializationPromise();
		chat = chatInstances.getOrCreate({
			conversationId: CHAT_ID,
			getModel: () => getDefaultModel()?.id ?? "",
			getNotebookId: () => null,
			getAgentMode: () => "interview",
		});
		// Resume: anything already said skips the door.
		try {
			const res = await fetch(`/api/chats/${CHAT_ID}`);
			if (res.ok) {
				const data = await res.json();
				const msgs = (data.messages ?? []).filter(
					(m: { role: string; content?: string }) =>
						(m.role === "user" || m.role === "assistant") && (m.content ?? "").trim(),
				);
				if (msgs.length) {
					chat.messages = msgs.map((m: { id: string; role: string; content: string }) => ({
						id: m.id,
						role: m.role,
						parts: [{ type: "text", text: m.content }],
					})) as unknown as typeof chat.messages;
					began = true;
				}
			}
		} catch {
			/* a fresh box has no chat yet — the door handles it */
		}
		loading = false;
	});

	onDestroy(() => {
		chatInstances.release(CHAT_ID);
	});

	async function send() {
		const text = draft.trim();
		if (!text || !chat || busy) return;
		draft = "";
		await chat.sendMessage({ text });
	}

	function onKeydown(e: KeyboardEvent) {
		// Enter is a newline — this is writing, not messaging. Cmd/Ctrl+Enter sets.
		if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			void send();
		}
	}
</script>

{#if loading}
	<p class="quiet">Finding the conversation…</p>
{:else if !began}
	<!-- ── THE DOOR ── -->
	<div>
		<h1 class="ob-h1">Sitting for a portrait</h1>
		<p class="ob-lede">
			Most onboarding asks you to fill out a profile. This is closer to sitting for a
			portrait: a conversation to fill in your past, understand your present, and explore
			your future — through the chapters of your life, the people who mattered, and the
			stories that made you. It becomes a document only you hold.
		</p>
		<p class="terms">
			It happens on your server, in your home — there is no cloud between you and it, and
			no one else who can read it. Nothing here will interpret you or press on anything
			you don't offer. Anything can be skipped, everything saves as you go, and only your
			words enter the document.
		</p>
		<p class="never">
			This will never be finished, and it isn't supposed to be. A record of a life can't
			be complete — there will always be another story worth telling. What matters is an
			honest start; the rest arrives over years, a question at a time.
		</p>

		<ul class="territories" aria-label="What it will ask about">
			<li>the chapters of your life</li>
			<li>what makes you unlike others</li>
			<li>who you admire</li>
			<li>what pulls at you</li>
			<li>what you believe</li>
		</ul>

		<button class="ob-btn" onclick={() => (began = true)}>
			Begin
			<Icon icon="ri:arrow-right-line" width="16" />
		</button>
	</div>
{:else}
	<!-- ── THE CONVERSATION ── -->
	<div class="manuscript">
		<div class="turn iv">
			<span class="mark">∴</span>
			<p>{OPENING}</p>
		</div>

		{#each chat?.messages ?? [] as m (m.id)}
			{@const text = textOf(m)}
			{#if text}
				{#if m.role === "assistant"}
					<div class="turn iv"><span class="mark">∴</span><p>{text}</p></div>
				{:else if m.role === "user"}
					<div class="turn me"><p>{text}</p></div>
				{/if}
			{/if}
		{/each}

		{#if chat?.status === "error"}
			<p class="ob-err">
				<Icon icon="ri:error-warning-line" width="14" /> That didn't reach your server —
				your words are still here. Try sending again.
			</p>
		{/if}

		<div class="writing">
			<textarea
				bind:value={draft}
				onkeydown={onKeydown}
				placeholder="Take your time."
				spellcheck="true"
			></textarea>
			<div class="writing-row">
				{#if spoke}
					<button class="finish" onclick={onfinish}>
						Write it up
						<Icon icon="ri:quill-pen-line" width="14" />
					</button>
				{:else}
					<span></span>
				{/if}
				<button class="set" onclick={send} disabled={busy || !draft.trim()} title="Send (⌘↵)">
					{busy ? "·" : "—"}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.terms {
		margin: 1.1rem 0 0;
		font-size: 0.95rem;
		line-height: 1.6;
		color: var(--color-foreground-muted);
		max-width: 38rem;
	}

	.never {
		margin: 1.6rem 0 0;
		padding-left: 1rem;
		border-left: 1px solid var(--color-border);
		font-size: 0.95rem;
		line-height: 1.6;
		color: var(--color-foreground-muted);
		max-width: 38rem;
	}

	.territories {
		margin: 1.8rem 0 0;
		padding: 0;
		list-style: none;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 11px;
		letter-spacing: 0.08em;
		color: var(--color-foreground-subtle);
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem 1.2rem;
	}

	.territories li::before {
		content: "· ";
	}

	/* ── the manuscript ── */
	.manuscript {
		display: flex;
		flex-direction: column;
	}

	.turn {
		margin-top: 1.9rem;
		max-width: 38rem;
	}

	.turn p {
		margin: 0;
		font-size: 1.02rem;
		line-height: 1.7;
	}

	/* The interviewer: marked with the ∴, a shade quieter. */
	.iv {
		display: flex;
		gap: 0.75rem;
	}

	.iv .mark {
		color: var(--color-primary);
		line-height: 1.7;
		flex: none;
	}

	.iv p {
		color: var(--color-foreground);
		font-size: 0.98rem;
	}

	/* Them: unmarked serif prose, indented under the mark's gutter. The page
	   should be mostly this. */
	.me {
		margin-left: 1.55rem;
	}

	.me p {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.05rem;
	}

	/* ── the writing surface ── */
	.writing {
		margin-top: 2.6rem;
		border-top: 1px solid var(--color-border);
		padding-top: 1.2rem;
	}

	textarea {
		width: 100%;
		min-height: 5.5rem;
		resize: vertical;
		border: none;
		background: none;
		outline: none;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.05rem;
		line-height: 1.65;
		color: var(--color-foreground);
	}

	.writing-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-top: 0.3rem;
	}

	.finish {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		background: none;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		padding: 0.45rem 1rem;
		font: inherit;
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}

	.finish:hover {
		color: var(--color-foreground);
		border-color: var(--color-foreground-subtle);
	}

	.set {
		background: none;
		border: none;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.35rem;
		line-height: 1;
		color: var(--color-foreground);
		cursor: pointer;
		padding: 0.2rem 0.4rem;
	}

	.set:disabled {
		color: var(--color-foreground-subtle);
		cursor: default;
	}

	.quiet {
		color: var(--color-foreground-subtle);
		font-size: 14px;
	}
</style>
