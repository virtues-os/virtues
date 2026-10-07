<!--
	DayEvidence.svelte

	The record behind one sentence of a day article, opened by clicking the
	sentence. Each source the writer cited shows its own words: a message as
	it was sent, a recording as the few turns nearest the sentence (lib/wiki/
	recordWords.ts). Nothing is summarized. A recording's turn is set darker
	only when it shares words with the sentence; otherwise the card says it is
	showing the opening turns. With the veil on, no words and no names show.
-->
<script lang="ts">
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import { portal } from "$lib/actions/portal";
	import { getRecord, type OntologyRecord } from "$lib/api/client";
	import type { MarginNote } from "$lib/wiki/dayArticle";
	import { messageBody, messageSender, nearTurns } from "$lib/wiki/recordWords";
	import { veil } from "$lib/stores/veil.svelte";

	interface Props {
		anchor: HTMLElement;
		evidence: MarginNote[];
		/** The sentence's markdown, veil marks kept: it places the recording's window. */
		sentence: string;
		/** What the card is about, for assistive tech: a sentence, or a table or photo. */
		label?: string;
		oncite: (ref: string) => void;
		/** Write in the margin beside this sentence. */
		onnote?: () => void;
		/** Close the card; `returnFocus` when the card was reached from the keyboard. */
		onclose: (returnFocus?: boolean) => void;
	}

	let { anchor, evidence, sentence, label = "The record behind this sentence", oncite, onnote, onclose }: Props = $props();

	let card: HTMLElement | null = $state(null);

	// Opened from the keyboard, the card takes focus so its buttons are the
	// next Tab; opened by a click, focus stays where the reader put it.
	const fromKeyboard = (() => {
		const active = document.activeElement;
		return !!active && (anchor === active || anchor.contains(active) || active.getAttribute("data-for") === "evidence");
	})();

	useClickOutside(() => [card, anchor], () => onclose());
	// Focus goes back only where it came from the keyboard, or is now inside
	// the card (it would otherwise fall to the page when the card goes).
	useEscapeKey(() => onclose(fromKeyboard || !!card?.contains(document.activeElement)));
	$effect(() => {
		if (card && fromKeyboard) card.focus({ preventScroll: true });
	});

	type Loaded = { note: MarginNote; record: OntologyRecord | null; failed: boolean };
	let sources = $state<Loaded[]>([]);

	$effect(() => {
		// One source per record: a sentence, or a table's lines, can cite the
		// same moment twice. A citation with no record (a chat) still shows.
		const notes = evidence.filter((n, i, all) => all.findIndex((m) => (m.ref ?? m.label) === (n.ref ?? n.label)) === i);
		sources = notes.map((note) => ({ note, record: note.ref ? (cache.get(note.ref) ?? null) : null, failed: false }));
		notes.forEach((note, i) => {
			const ref = note.ref;
			if (!ref || cache.has(ref)) return;
			const [table, ...rest] = ref.split(":");
			getRecord(table, rest.join(":"))
				.then((r) => {
					cache.set(ref, r);
					if (sources[i]?.note === note) sources[i] = { note, record: r, failed: false };
				})
				.catch(() => {
					if (sources[i]?.note === note) sources[i] = { note, record: null, failed: true };
				});
		});
	});

	const tableOf = (n: MarginNote) => (n.ref ?? "").split(":")[0];
</script>

<script lang="ts" module>
	/** Records already fetched this session, by `table:id`. */
	const cache = new Map<string, OntologyRecord>();
</script>

<div use:portal>
	<FloatingContent {anchor} options={{ placement: "bottom-start", offset: 8, flip: true, shift: true, padding: 12, strategy: "fixed" }}>
		<div class="evidence-card" bind:this={card} role="dialog" aria-label={label} tabindex="-1">
			{#each sources as s (s.note.ref ?? s.note.label)}
				{@const table = tableOf(s.note)}
				<section class="source">
					<p class="kick">
						<span>
							<b>{s.note.label.replace(/\s*·\s*$/, "")}</b>{#if table === "data_communication_transcription"} · voices unnamed{/if}{#if s.record && table === "data_communication_message" && !veil.hiding} · {messageSender(s.record.row)}{/if}
						</span>
						{#if s.note.ref}
							<button type="button" class="open" onclick={() => oncite(s.note.ref as string)}>Open in Data <span aria-hidden="true">↗</span></button>
						{/if}
					</p>
					{#if !s.note.ref}
						<p class="quiet">A question you asked Virtues that day. You'll find the day's chats in Data.</p>
					{:else if veil.hiding}
						<p class="quiet">Veiled. Hold V to read the record.</p>
					{:else if s.failed}
						<p class="quiet">Couldn't load this record. Check that your server is running, then open it again.</p>
					{:else if !s.record}
						<p class="quiet">Loading…</p>
					{:else if table === "data_communication_message"}
						<blockquote><p>{messageBody(s.record.row) || "(no text)"}</p></blockquote>
					{:else if table === "data_communication_transcription"}
						{@const near = nearTurns(String(s.record.row.text ?? ""), sentence)}
						<blockquote>
							{#each near.turns as turn, i (i)}
								<p class:near={turn.near}>{turn.text}</p>
							{/each}
						</blockquote>
						{#if !near.matched}<p class="quiet">The sentence's words aren't in this recording's text, so these are its opening turns.</p>{/if}
					{:else}
						<p class="quiet">{s.record.display_name}</p>
					{/if}
				</section>
			{/each}
			{#if onnote}
				<p class="actions"><button type="button" class="open" onclick={() => onnote?.()}>Write a note</button></p>
			{/if}
		</div>
	</FloatingContent>
</div>

<style>
	.evidence-card {
		width: min(24rem, calc(100vw - 32px));
		max-height: min(26rem, 60vh);
		overflow-y: auto;
		padding: 0.75rem 0.875rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.evidence-card:focus {
		outline: none;
	}

	.source + .source {
		border-top: 1px solid var(--color-border-subtle);
		margin-top: 0.625rem;
		padding-top: 0.625rem;
	}

	.kick {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		gap: 0.75rem;
		margin: 0;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.kick b {
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	.open {
		flex: none;
		font: inherit;
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	.open:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	blockquote {
		margin: 0.375rem 0 0;
		font-family: var(--font-serif);
		font-size: 1rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}

	blockquote p {
		margin: 0;
	}

	blockquote p + p {
		margin-top: 0.375rem;
	}

	/* The turn that shares the sentence's words, when one does; its neighbors
	   set it in context. A message is one turn and reads at full strength. */
	blockquote p.near,
	blockquote p:only-child {
		color: var(--color-foreground);
	}

	.actions {
		margin: 0.75rem 0 0;
		padding-top: 0.625rem;
		border-top: 1px solid var(--color-border-subtle);
		font-size: 0.8125rem;
	}

	.quiet {
		margin: 0.375rem 0 0;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}
</style>
