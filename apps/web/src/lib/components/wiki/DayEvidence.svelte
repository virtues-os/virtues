<!--
	DayEvidence.svelte

	The record behind one sentence of a day article, opened by clicking the
	sentence. Each source the writer cited shows its own words: a message as
	it was sent, a recording as the few turns nearest the sentence. Nothing is
	summarized and nothing is marked unless the record itself says so.

	A passage the writer veiled as hard (a lowercase phrase in ⟦ ⟧, as opposed
	to a name) keeps its source folded until you ask for the words. With the
	veil on, no words show at all.
-->
<script lang="ts">
	import { portal } from "$lib/actions/portal";
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import { getRecord, type OntologyRecord } from "$lib/api/client";
	import type { MarginNote } from "$lib/wiki/dayArticle";
	import { veil } from "$lib/stores/veil.svelte";

	interface Props {
		anchor: HTMLElement;
		evidence: MarginNote[];
		/** The sentence's markdown, veil marks kept: it places the recording's window. */
		sentence: string;
		oncite: (ref: string) => void;
		onclose: () => void;
	}

	let { anchor, evidence, sentence, oncite, onclose }: Props = $props();

	let card: HTMLElement | null = $state(null);
	useClickOutside(() => [card, anchor], () => onclose());
	useEscapeKey(() => onclose());

	/** A hard passage, not just a name: the writer veiled a lowercase phrase. */
	const sensitive = $derived([...sentence.matchAll(/⟦([^⟧]*)⟧/g)].some((m) => /^[a-z]/.test(m[1].trim()) && /\s/.test(m[1].trim())));
	let shown = $state(false);

	type Loaded = { note: MarginNote; record: OntologyRecord | null; failed: boolean };
	let sources = $state<Loaded[]>([]);

	$effect(() => {
		const notes = evidence.filter((n) => n.ref);
		sources = notes.map((note) => ({ note, record: cache.get(note.ref as string) ?? null, failed: false }));
		notes.forEach((note, i) => {
			const ref = note.ref as string;
			if (cache.has(ref)) return;
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

	const STOP = new Set("the and you your was were that this with for from had have her his she they them then there what when who into out about just like said told asked".split(" "));
	const words = (s: string) =>
		new Set(
			s
				.toLowerCase()
				.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
				.replace(/[⟦⟧]/g, "")
				.match(/[a-z0-9']{3,}/g)
				?.filter((w) => !STOP.has(w)) ?? [],
		);

	/** A recording's turns, and the window nearest the sentence. */
	function turnsOf(text: string): { turns: string[]; best: number } {
		const turns = text
			.split(/\[Speaker(?: \d+)?\]:\s*/)
			.map((t) => t.trim())
			.filter(Boolean);
		const want = words(sentence);
		let best = 0;
		let score = 0;
		turns.forEach((t, i) => {
			let s = 0;
			for (const w of words(t)) if (want.has(w)) s += 1;
			if (s > score) [best, score] = [i, s];
		});
		return { turns, best };
	}

	function window3(turns: string[], best: number): { text: string; centre: boolean }[] {
		const from = Math.max(0, best - 1);
		return turns.slice(from, from + 4).map((text, i) => ({ text, centre: from + i === best }));
	}

	function who(row: Record<string, unknown>): string {
		const meta = (row.metadata ?? {}) as Record<string, unknown>;
		const fromMe = meta.is_from_me === true || meta.is_from_me === "true";
		const group = row.is_group_message === true ? (meta.group_title as string | undefined) : undefined;
		const sender = fromMe ? "You" : ((row.from_name as string | undefined) ?? "Someone");
		return group ? `${sender}, in ${group}` : sender;
	}

	function messageBody(row: Record<string, unknown>): string {
		const body = String(row.body ?? "").replace(/￼/g, "").trim();
		return body.length > 420 ? `${body.slice(0, 420)}…` : body;
	}
</script>

<script lang="ts" module>
	/** Records already fetched this session, by `table:id`. */
	const cache = new Map<string, OntologyRecord>();
</script>

<div use:portal>
	<FloatingContent {anchor} options={{ placement: "bottom-start", offset: 8, flip: true, shift: true, padding: 12, strategy: "fixed" }}>
		<div class="evidence-card" bind:this={card} role="dialog" aria-label="The record behind this sentence">
			{#each sources as s (s.note.ref)}
				{@const table = (s.note.ref ?? "").split(":")[0]}
				<section class="source">
					<p class="kick">
						<span>
							<b>{s.note.label}</b>{#if table === "data_communication_transcription"} · voices unnamed{/if}{#if s.record && table === "data_communication_message"} · {who(s.record.row)}{/if}
						</span>
						<button type="button" class="open" onclick={() => oncite(s.note.ref as string)}>Open in Data <span aria-hidden="true">↗</span></button>
					</p>
					{#if veil.hiding}
						<p class="quiet">Veiled. Hold V to read the record.</p>
					{:else if sensitive && !shown}
						<p class="quiet">A hard passage, so its words stay folded. <button type="button" class="reveal" onclick={() => (shown = true)}>Show the words</button></p>
					{:else if s.failed}
						<p class="quiet">Couldn't load this record. Open it in Data to try again.</p>
					{:else if !s.record}
						<p class="quiet">Loading…</p>
					{:else if table === "data_communication_message"}
						<blockquote><p>{messageBody(s.record.row) || "(no text)"}</p></blockquote>
					{:else if table === "data_communication_transcription"}
						{@const t = turnsOf(String(s.record.row.text ?? ""))}
						<blockquote>
							{#each window3(t.turns, t.best) as turn, i (i)}
								<p class:centre={turn.centre}>{turn.text}</p>
							{/each}
						</blockquote>
					{:else}
						<p class="quiet">{s.record.display_name}</p>
					{/if}
				</section>
			{/each}
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

	.open,
	.reveal {
		flex: none;
		font: inherit;
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	.open:hover,
	.reveal:hover {
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

	/* The turn that shares the sentence's words; its neighbours set it in context. */
	blockquote p.centre,
	blockquote p:only-child {
		color: var(--color-foreground);
	}

	.quiet {
		margin: 0.375rem 0 0;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}
</style>
