<!--
  The draft, handed back for correction.

  The interview's last screen calls this, and this is where the person first
  sees what their answers became. It is a MIRROR: it arranged what they wrote,
  and everything about the presentation should say so — their words, their
  headings, and an edit box rather than a wall of finished prose with an OK
  button.

  RULES ARE PROPOSED, NEVER APPLIED. The model reads the last answer and
  suggests short imperatives; nothing binds the assistant until it is ticked
  here. A rule the box invented and then obeyed would be worse than having no
  rules at all, because it would be invisible and permanent — the person would
  never learn why it stopped mentioning their brother.

  Unticked by default for the same reason. Consent that has to be withdrawn is
  not consent.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { draftNarrative, saveNarrativeRules, type NarrativeDraft } from "$lib/api/client";
	import { onMount } from "svelte";

	let { ondone, reduced = false }: { ondone: () => void; reduced?: boolean } = $props();

	let draft = $state<NarrativeDraft | null>(null);
	let error = $state<string | null>(null);
	let busy = $state(true);
	let chosen = $state<Record<number, boolean>>({});
	let edited = $state<Record<number, string>>({});
	let saving = $state(false);

	onMount(async () => {
		try {
			draft = await draftNarrative();
			draft.proposed_rules.forEach((r, i) => (edited[i] = r));
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		}
		busy = false;
	});

	async function finish() {
		saving = true;
		try {
			const rules = (draft?.proposed_rules ?? [])
				.map((_, i) => (chosen[i] ? (edited[i] ?? "").trim() : ""))
				.filter(Boolean);
			// Always called, even with none chosen — an empty confirmed set is a
			// real answer and has to overwrite whatever was there before.
			await saveNarrativeRules(rules);
		} catch (e) {
			// Never trap someone here over the rules write. The document is
			// already saved; this is the smaller half.
			error = e instanceof Error ? e.message : String(e);
		}
		saving = false;
		ondone();
	}
</script>

<div class="wrap" class:still={reduced}>
	<div class="sheet">
		{#if busy}
			<p class="quiet">
				<Icon icon="ri:quill-pen-line" width="15" /> Reading what you wrote…
			</p>
		{:else if error && !draft}
			<h1>That didn't come together</h1>
			<p class="quiet">{error}</p>
			<p class="quiet">
				Nothing is lost — every answer is saved, and this can be tried again from your
				document.
			</p>
			<button class="next" onclick={ondone}>Carry on</button>
		{:else if draft}
			<p class="kicker">In your own words</p>
			<h1>Here's what you said</h1>
			<p class="lede">
				Arranged from your answers, in your words. It will be wrong in places — it is a
				draft of you, written by a machine that only knows what you just told it. Correct
				it whenever you like; nothing here is fixed.
			</p>

			<article class="doc">{draft.document}</article>

			{#if draft.proposed_rules.length}
				<section class="rules">
					<h2>Things you asked it never to raise</h2>
					<p class="quiet">
						These become rules rather than context — the box will obey them rather than
						weigh them. Tick the ones you meant, and edit the wording; nothing here is
						in effect until you do.
					</p>

					{#each draft.proposed_rules as _, i}
						<label class="rule">
							<input type="checkbox" bind:checked={chosen[i]} />
							<input class="rule-text" type="text" bind:value={edited[i]} />
						</label>
					{/each}
				</section>
			{/if}

			{#if error}
				<p class="err"><Icon icon="ri:error-warning-line" width="14" /> {error}</p>
			{/if}

			<button class="next" onclick={finish} disabled={saving}>
				{saving ? "Saving…" : "Done"}
				<Icon icon="ri:arrow-right-line" width="15" />
			</button>
		{/if}
	</div>
</div>

<style>
	.wrap {
		min-height: 100vh;
		display: flex;
		align-items: flex-start;
		justify-content: center;
		padding: 4rem 1.5rem;
	}

	.sheet {
		width: 100%;
		max-width: 40rem;
		animation: rise 0.5s ease both;
	}

	.still .sheet {
		animation: none;
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(8px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	.kicker {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 11px;
		letter-spacing: 0.16em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
		margin: 0 0 0.6rem;
	}

	h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: clamp(1.6rem, 3vw, 2.1rem);
		margin: 0;
		line-height: 1.15;
	}

	.lede {
		margin: 1rem 0 0;
		font-size: 14px;
		line-height: 1.65;
		color: var(--color-foreground-muted);
		max-width: 34rem;
	}

	/* Reads as a document, not a result. `pre-wrap` keeps the model's paragraph
	   breaks without pulling a markdown renderer into this screen. */
	.doc {
		margin-top: 2rem;
		padding: 1.5rem 1.6rem;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
		white-space: pre-wrap;
		font-size: 1rem;
		line-height: 1.7;
	}

	.rules {
		margin-top: 2.25rem;
	}

	h2 {
		font-size: 15px;
		font-weight: 600;
		margin: 0 0 0.5rem;
	}

	.rule {
		display: flex;
		align-items: center;
		gap: 0.65rem;
		margin-top: 0.6rem;
	}

	.rule-text {
		flex: 1;
		font: inherit;
		font-size: 14px;
		padding: 0.5rem 0.7rem;
		border-radius: 8px;
		border: 1px solid var(--color-border);
		background: none;
		color: var(--color-foreground);
	}

	.quiet {
		font-size: 13.5px;
		line-height: 1.6;
		color: var(--color-foreground-subtle);
		max-width: 34rem;
	}

	.err {
		margin-top: 1rem;
		font-size: 13px;
		color: #ff9ea1;
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.next {
		margin-top: 2.25rem;
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		font: inherit;
		font-size: 15px;
		padding: 0.65rem 1.3rem;
		border-radius: 10px;
		border: 1px solid var(--color-border);
		background: none;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.next:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
	}

	.next:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
