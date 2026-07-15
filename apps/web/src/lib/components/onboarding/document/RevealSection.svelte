<!--
  RevealSection — the payoff: the box's first draft of your narrative identity.

  When the section is reached, it asks the box to draft (POST the generator
  action) and waits. The shell polls /api/setup/state; once the portrait exists
  (`ready`), it fetches and streams the prose in, closed by a colophon. Until
  then it's an honest "being written…" — and if the data's too thin the draft
  defers, so the promise simply stands until more arrives.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import { fly } from "svelte/transition";
	import { expoOut } from "svelte/easing";
	import Icon from "$lib/components/Icon.svelte";
	import { Button } from "$lib";
	import Markdown from "$lib/components/Markdown.svelte";
	import { triggerAction, getNarrativeIdentity } from "$lib/api/client";
	import { formatDate } from "$lib/utils/dateUtils";

	interface Props {
		/** A non-empty portrait exists (from /api/setup/state). */
		ready: boolean;
		/** A draft run is currently in flight. */
		generating?: boolean;
		reduced?: boolean;
		onEnter: () => void;
	}

	let { ready, generating = false, reduced = false, onEnter }: Props = $props();

	let content = $state("");
	let triggered = false;
	let triggerFailed = $state(false);

	const today = formatDate(new Date(), {
		day: "numeric",
		month: "long",
		year: "numeric",
	});

	async function triggerDraft() {
		if (triggered) return;
		triggered = true;
		triggerFailed = false;
		try {
			await triggerAction("action_narrative_identity_draft");
		} catch {
			triggerFailed = true;
		}
	}

	async function loadContent() {
		try {
			const d = await getNarrativeIdentity<{ content?: string }>();
			content = (d.content ?? "").trim();
		} catch {
			/* keep waiting */
		}
	}

	// Kick off a draft the moment we land here (unless one's already ready).
	onMount(() => {
		if (!ready) void triggerDraft();
	});

	// When the portrait becomes ready, pull it in once.
	$effect(() => {
		if (ready && !content) void loadContent();
	});

	function redraft() {
		content = "";
		triggered = false;
		void triggerDraft();
	}
</script>

{#if ready && content}
	<div in:fly={{ y: reduced ? 0 : 14, duration: reduced ? 0 : 520, easing: expoOut }}>
		<p class="dateline">Generated on your box · {today}</p>
		<div class="portrait">
			<Markdown {content} isStreaming={!reduced} />
		</div>
		<p class="colophon">Generated on your box from your connected data. Stored only here.</p>
		<p class="draft-note">This is a first draft from limited data. Edit anything that's wrong — it's yours.</p>
		<div class="cta">
			<Button variant="primary" class="px-5 py-2.5" onclick={onEnter}>This is me — enter Virtues</Button>
			<button class="redraft" onclick={redraft}>Redraft</button>
		</div>
	</div>
{:else}
	<div class="pending">
		<Icon icon="ri:quill-pen-line" width="18" class={generating || !triggerFailed ? "pen" : ""} />
		{#if triggerFailed}
			<span>Your narrative identity will be drafted once your box is set up. It'll appear here when it's ready.</span>
		{:else}
			<span>Your narrative identity is being written from your data. It'll appear here when it's ready.</span>
		{/if}
	</div>
	<button class="enter" onclick={onEnter}>Enter Virtues →</button>
{/if}

<style>
	@reference "../../../../app.css";

	.dateline {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		letter-spacing: 0.04em;
		color: var(--color-foreground-subtle);
		margin: 0 0 1.5rem;
	}

	.portrait {
		font-family: var(--font-serif);
		font-size: 1.2rem;
		line-height: 1.7;
		color: var(--color-foreground);
		max-width: 34rem;
	}
	/* Keep the portrait prose calm — override the .markdown heading/spacing chrome. */
	.portrait :global(.markdown p) {
		margin-bottom: 0;
		font-size: inherit;
		line-height: inherit;
	}

	.colophon {
		margin: 1.75rem 0 0;
		font-family: var(--font-mono);
		font-size: 0.72rem;
		letter-spacing: 0.01em;
		color: var(--color-foreground-subtle);
	}

	.draft-note {
		margin: 0.6rem 0 0;
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
	}

	.cta {
		margin-top: 2rem;
		display: flex;
		align-items: center;
		gap: 1.25rem;
	}
	.redraft {
		font-size: 0.85rem;
		color: var(--color-foreground-subtle);
		transition: color 0.15s ease;
	}
	.redraft:hover {
		color: var(--color-foreground);
	}

	.pending {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		color: var(--color-foreground-muted);
		font-size: 1.0625rem;
	}
	.enter {
		margin-top: 1.75rem;
		font-size: 0.95rem;
		font-weight: 500;
		color: var(--color-foreground);
		transition: color 0.15s ease;
	}
	.enter:hover {
		color: var(--color-primary);
	}
	:global(.pen) {
		animation: pen-pulse 1.8s ease-in-out infinite;
	}
	@keyframes pen-pulse {
		0%,
		100% {
			opacity: 0.45;
		}
		50% {
			opacity: 1;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		:global(.pen) {
			animation: none;
		}
	}
</style>
