<!--
  The chapters question, as cards instead of a blank page.

  WHY THIS ONE QUESTION GETS STRUCTURE when questions.ts banned schema-fields
  ("PROSE, NOT FIELDS"): chapters are the one answer that is naturally a list —
  discrete, ordered, countable — and the blank textarea demonstrably tangles
  places, dates and changepoints into one paragraph. Both of the doctrine's
  objections dissolve here: an "add a chapter" list has no empty fields to nag,
  and THE STORED ARTIFACT STAYS PROSE — cards serialize to readable text in the
  same answer field, so the drafter and the word meter read it exactly as
  before.

  A CHAPTER READS AS A LINE, NOT A FORM. The first version stacked four
  gray input boxes per card and the page read as bureaucracy ("ugly and
  confusing... inundating" — the founder, immediately). Now: two quiet lines —
  a serif title with rough years beside it, and what ended it beneath — on a
  hairline card, inputs invisible until touched. White-on-white; the type is
  the interface.

  The serialization is the contract: one block per chapter —

      — The Wisconsin years (2005–2015)
      Ended when: left for college

  — plus an optional final "Also:" block for what fits no chapter. The "— "
  head marker is the format sentinel: prose never starts every paragraph with
  it, so an answer written before the cards existed is detected and NEVER
  clobbered — it lands whole in the Also field, which stays collapsed until it
  has something to show.
-->
<script lang="ts">
	interface Chapter {
		name: string;
		years: string;
		ended: string;
	}

	let { value, onchange }: { value: string; onchange: (text: string) => void } = $props();

	// Ghost text only — scent, never template. Cycled by card index.
	const NAME_EXAMPLES = [
		"Childhood",
		"The Wisconsin years",
		"College",
		"The first job",
		"After the move",
		"The hard year",
		"Early career",
		"Since the diagnosis",
	];

	function parse(text: string): { chapters: Chapter[]; also: string } {
		const t = text.trim();
		if (!t) return { chapters: [{ name: "", years: "", ended: "" }], also: "" };
		const blocks = t.split(/\n\s*\n/);
		// Everything from the first "Also:" block onward is the free text — it
		// can itself contain blank lines (a preserved pre-cards answer often
		// does), so it must not be re-split into pseudo-blocks and re-validated.
		const alsoIdx = blocks.findIndex((b) => b.trimStart().startsWith("Also:"));
		const chapterBlocks = alsoIdx === -1 ? blocks : blocks.slice(0, alsoIdx);
		const also =
			alsoIdx === -1
				? ""
				: blocks.slice(alsoIdx).join("\n\n").replace(/^Also:\s*/, "").trim();
		const chapters: Chapter[] = [];
		for (const block of chapterBlocks) {
			const lines = block.split("\n").map((l) => l.trim()).filter(Boolean);
			const rest = lines.slice(1);
			// Ours iff the head carries the "— " sentinel AND every following
			// line is a known field ("About:" accepted for reading old saves).
			// Anything else predates the cards and survives untouched in Also.
			if (
				!lines[0]?.startsWith("— ") ||
				!rest.every((l) => l.startsWith("Ended when:") || l.startsWith("About:"))
			) {
				return { chapters: [{ name: "", years: "", ended: "" }], also: t };
			}
			const head = lines[0].slice(2);
			const m = head.match(/^(.*?)\s*\(([^)]*)\)\s*$/);
			chapters.push({
				name: (m ? m[1] : head).trim(),
				years: (m ? m[2] : "").trim(),
				ended: rest.find((l) => l.startsWith("Ended when:"))?.slice("Ended when:".length).trim() ?? "",
			});
		}
		if (chapters.length === 0) chapters.push({ name: "", years: "", ended: "" });
		return { chapters, also };
	}

	// Parsed ONCE, on mount, on purpose: the cards own the text from here and
	// the parent re-keys this component when the question changes. Reacting to
	// `value` would fight the person's own typing.
	// svelte-ignore state_referenced_locally
	const initial = parse(value);
	let chapters = $state<Chapter[]>(initial.chapters);
	let also = $state(initial.also);
	// svelte-ignore state_referenced_locally
	let alsoOpen = $state(initial.also.trim().length > 0);

	function serialize(): string {
		const blocks = chapters
			.filter((c) => c.name.trim() || c.years.trim() || c.ended.trim())
			.map((c) => {
				const head = c.years.trim() ? `— ${c.name.trim()} (${c.years.trim()})` : `— ${c.name.trim()}`;
				return c.ended.trim() ? `${head}\nEnded when: ${c.ended.trim()}` : head;
			});
		if (also.trim()) blocks.push(`Also: ${also.trim()}`);
		return blocks.join("\n\n");
	}

	function changed() {
		onchange(serialize());
	}

	function add() {
		chapters.push({ name: "", years: "", ended: "" });
	}

	function remove(i: number) {
		chapters.splice(i, 1);
		if (chapters.length === 0) add();
		changed();
	}
</script>

<div class="cards">
	{#each chapters as c, i}
		<div class="card">
			<div class="head">
				<input
					class="name"
					bind:value={c.name}
					oninput={changed}
					placeholder={NAME_EXAMPLES[i % NAME_EXAMPLES.length]}
					aria-label="Chapter name"
				/>
				<input
					class="years"
					bind:value={c.years}
					oninput={changed}
					placeholder="rough years"
					aria-label="Rough years"
				/>
				<button class="rm" onclick={() => remove(i)} aria-label="Remove chapter">×</button>
			</div>
			<input
				class="ended"
				bind:value={c.ended}
				oninput={changed}
				placeholder="what ended it"
				aria-label="What ended it"
			/>
		</div>
	{/each}

	<div class="row">
		<button class="add" onclick={add}>+ Add a chapter</button>
		{#if !alsoOpen}
			<button class="add quiet" onclick={() => (alsoOpen = true)}>+ The rest</button>
		{/if}
	</div>

	{#if alsoOpen}
		<label class="also">
			<span>Whatever fits no chapter</span>
			<textarea bind:value={also} oninput={changed} rows="3"></textarea>
		</label>
	{/if}
</div>

<style>
	.cards {
		margin-top: 1.5rem;
		display: flex;
		flex-direction: column;
		gap: 0.7rem;
	}

	/* One hairline holds the chapter; everything inside is bare type. */
	.card {
		border: 1px solid var(--color-border-subtle, var(--color-border));
		border-radius: 12px;
		padding: 0.85rem 1.05rem 0.9rem;
	}

	.head {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
	}

	/* Invisible inputs: no box, no background — the type is the interface. */
	input {
		border: none;
		background: none;
		padding: 0;
		color: var(--color-foreground);
	}

	input:focus {
		outline: none;
	}

	input::placeholder {
		color: var(--color-foreground-subtle);
		opacity: 0.7;
	}

	/* The chapter's title, set like one. */
	.name {
		flex: 1;
		min-width: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.15rem;
	}

	.years {
		width: 8.5rem;
		text-align: right;
		font-size: 0.85rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}

	.ended {
		width: 100%;
		margin-top: 0.35rem;
		font-size: 0.9rem;
		color: var(--color-foreground-muted);
	}

	.rm {
		border: none;
		background: none;
		color: transparent;
		font-size: 1rem;
		line-height: 1;
		cursor: pointer;
		padding: 0 0.1rem;
	}

	.card:hover .rm,
	.card:focus-within .rm {
		color: var(--color-foreground-subtle);
	}

	.rm:hover {
		color: var(--color-foreground);
	}

	.row {
		display: flex;
		gap: 0.9rem;
		align-items: center;
	}

	.add {
		border: none;
		background: none;
		padding: 0.2rem 0;
		font-size: 13.5px;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}

	.add:hover {
		color: var(--color-foreground);
	}

	.add.quiet {
		color: var(--color-foreground-subtle);
	}

	.also {
		display: flex;
		flex-direction: column;
		gap: 0.4rem;
		font-size: 13px;
		color: var(--color-foreground-subtle);
	}

	.also textarea {
		width: 100%;
		resize: vertical;
		background: color-mix(in srgb, var(--color-foreground) 3%, transparent);
		border: 1px solid var(--color-border);
		border-radius: 10px;
		padding: 0.7rem 0.85rem;
		font: inherit;
		font-size: 0.95rem;
		line-height: 1.6;
		color: var(--color-foreground);
	}

	.also textarea:focus {
		outline: none;
		border-color: color-mix(in srgb, var(--color-primary) 60%, transparent);
	}
</style>
