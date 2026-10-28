<script lang="ts">
	/**
	 * The rules in force (`wiki_rules`, active): what chat never raises
	 * (`avoid`) and what it holds the person to (`defend`), also read by the
	 * wiki editor. The interview proposes rules; this is where the ones in
	 * force are listed, added, edited and removed.
	 *
	 * Every save replaces the whole set, so it always sends `keepProposed`:
	 * proposals waiting for the interview's close are not this page's to drop.
	 */
	import { onDestroy, onMount, tick } from 'svelte';
	// By path, not the `$lib` barrel, which the component tests cannot load.
	import Button from '$lib/components/Button.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Input from '$lib/components/Input.svelte';
	import { getNarrativeRules, saveNarrativeRules, type NarrativeRule } from '$lib/api/client';

	type Kind = NarrativeRule['kind'];

	interface Row {
		/** Local only: the server renumbers ids on every save. */
		key: number;
		rule: string;
		kind: Kind;
		/** Removed, and waiting out its seconds of Undo. */
		removing: boolean;
	}

	const KIND_LABEL: Record<Kind, string> = {
		avoid: 'Never bring up',
		defend: 'Hold me to',
	};
	const PLACEHOLDER: Record<Kind, string> = {
		avoid: 'my ex',
		defend: 'no work email on Sundays',
	};
	const UNDO_MS = 6000;

	let { assistantName = 'Ari' }: { assistantName?: string } = $props();

	let rows = $state<Row[]>([]);
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let saveFailed = $state(false);
	let editingKey = $state<number | null>(null);
	let editDraft = $state('');
	let addKind = $state<Kind>('avoid');
	let addText = $state('');
	let listEl = $state<HTMLElement>();
	let addEl = $state<HTMLElement>();

	let nextKey = 0;
	const timers = new Map<number, ReturnType<typeof setTimeout>>();
	/** The last set the server confirmed, for when it can't be asked again. */
	let confirmed: { rule: string; kind: Kind }[] = [];
	// A save replaces the whole set, so saves run one at a time and each sends
	// the list as it stands when its turn comes.
	let queue: Promise<void> = Promise.resolve();
	// Bumped when a failed save puts the server's list back, so a save asked
	// for before that does not send a list the person no longer sees.
	let generation = 0;

	const ready = $derived(phase === 'ready');

	function toRows(rules: { rule: string; kind: Kind }[]): Row[] {
		return rules.map((r) => ({
			key: nextKey++,
			rule: r.rule,
			kind: r.kind === 'defend' ? 'defend' : 'avoid',
			removing: false,
		}));
	}

	function plain(list: { rule: string; kind: Kind }[]) {
		return list.map(({ rule, kind }) => ({ rule, kind }));
	}

	async function load() {
		phase = 'loading';
		try {
			const { rules } = await getNarrativeRules();
			rows = toRows(rules);
			confirmed = plain(rows);
			phase = 'ready';
		} catch {
			phase = 'error';
		}
	}

	onMount(() => {
		void load();
	});

	function save(onFail?: () => void) {
		const asked = generation;
		queue = queue.then(async () => {
			if (asked !== generation) return;
			const sending = plain(rows);
			try {
				await saveNarrativeRules(sending, { keepProposed: true });
				confirmed = sending;
				saveFailed = false;
			} catch {
				generation++;
				onFail?.();
				await showServerList();
				saveFailed = true;
			}
		});
	}

	/** After a failed save, the list goes back to what the server has. */
	async function showServerList() {
		for (const t of timers.values()) clearTimeout(t);
		timers.clear();
		editingKey = null;
		try {
			confirmed = plain((await getNarrativeRules()).rules);
		} catch {
			// The save failed, so the server still has the last set it confirmed.
		}
		rows = toRows(confirmed);
	}

	function rowEl(key: number) {
		return listEl?.querySelector<HTMLElement>(`[data-key="${key}"]`);
	}

	function focusIn(key: number, selector: string) {
		rowEl(key)?.querySelector<HTMLElement>(selector)?.focus();
	}

	async function startEdit(row: Row) {
		editingKey = row.key;
		editDraft = row.rule;
		await tick();
		focusIn(row.key, 'input');
	}

	function finishEdit(key: number) {
		if (editingKey !== key) return;
		editingKey = null;
		const row = rows.find((r) => r.key === key);
		const text = editDraft.trim();
		// Emptying a rule is not removing it; Remove does that, with an Undo.
		if (!row || !text || text === row.rule) return;
		row.rule = text;
		save();
	}

	async function editKeydown(e: KeyboardEvent, key: number) {
		if (e.key === 'Enter') {
			e.preventDefault();
			finishEdit(key);
		} else if (e.key === 'Escape') {
			e.preventDefault();
			editingKey = null;
		} else {
			return;
		}
		await tick();
		focusIn(key, 'button[aria-label="Edit rule"]');
	}

	async function remove(row: Row) {
		row.removing = true;
		timers.set(
			row.key,
			setTimeout(() => commitRemovals([row.key]), UNDO_MS),
		);
		await tick();
		focusIn(row.key, 'button');
	}

	async function undo(row: Row) {
		clearTimeout(timers.get(row.key));
		timers.delete(row.key);
		row.removing = false;
		await tick();
		focusIn(row.key, 'button[aria-label="Remove rule"]');
	}

	async function commitRemovals(keys: number[]) {
		for (const key of keys) {
			clearTimeout(timers.get(key));
			timers.delete(key);
		}
		const gone = (r: Row) => keys.includes(r.key);
		if (!rows.some(gone)) return;

		// If the row leaving held focus (its Undo), focus moves to a neighbour
		// rather than falling to the page.
		const at = rows.findIndex((r) => gone(r) && rowEl(r.key)?.contains(document.activeElement));
		const neighbour =
			at < 0
				? undefined
				: (rows.slice(at + 1).find((r) => !gone(r)) ??
					rows.slice(0, at).reverse().find((r) => !gone(r)));
		rows = rows.filter((r) => !gone(r));
		save();
		if (at < 0) return;
		await tick();
		if (neighbour) focusIn(neighbour.key, 'button');
		else addEl?.querySelector<HTMLElement>('input')?.focus();
	}

	// Leaving the page commits a removal still waiting out its Undo, rather
	// than bringing the rule back.
	function commitPending() {
		if (timers.size) void commitRemovals([...timers.keys()]);
	}

	onDestroy(commitPending);

	function add() {
		const text = addText.trim();
		if (!text || !ready) return;
		const kind = addKind;
		rows.push({ key: nextKey++, rule: text, kind, removing: false });
		addText = '';
		save(() => {
			// Give the words back, unless something new has been typed since.
			if (!addText) {
				addText = text;
				addKind = kind;
			}
		});
	}

	function addKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			add();
		}
	}
</script>

<svelte:window onpagehide={commitPending} />

<section class="rules">
	<h2 class="settings-label">Rules</h2>
	<!-- What a rule does in chat (RULES_PROMPT in agent/prompt.rs): never
	     opens the subject, still engages when the owner does. -->
	<p class="sec-hint">
		{assistantName} follows these in chat.
		It never raises a subject you've ruled out, but talks about it if you do.
	</p>

	{#if phase === 'error'}
		<p class="field-error">Your server couldn't load the rules.</p>
		<div>
			<Button variant="secondary" size="sm" onclick={load}>Try again</Button>
		</div>
	{:else}
		{#if rows.length}
			<ul class="list" bind:this={listEl}>
				{#each rows as row (row.key)}
					<li class="row" data-key={row.key}>
						{#if row.removing}
							<span class="removed" role="status">Removed.</span>
							<span class="actions">
								<Button variant="ghost" size="sm" onclick={() => undo(row)}>Undo</Button>
							</span>
						{:else}
							<span class="kind">{KIND_LABEL[row.kind]}</span>
							{#if editingKey === row.key}
								<label class="edit">
									<span class="sr-only">Rule</span>
									<Input
										bind:value={editDraft}
										autocomplete="off"
										onkeydown={(e) => editKeydown(e, row.key)}
										onblur={() => finishEdit(row.key)}
									/>
								</label>
							{:else}
								<span class="text">{row.rule}</span>
								<span class="actions">
									<IconButton
										icon="mynaui:edit"
										label="Edit rule"
										size="sm"
										onclick={() => startEdit(row)}
									/>
									<IconButton
										icon="mynaui:x"
										label="Remove rule"
										size="sm"
										onclick={() => remove(row)}
									/>
								</span>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
		{/if}

		{#if saveFailed}
			<p class="field-error" role="alert">
				Your server couldn't save the rules. They're as they were. Try again in a moment.
			</p>
		{/if}

		<div class="add" bind:this={addEl}>
			<select bind:value={addKind} aria-label="Kind of rule" disabled={!ready}>
				<option value="avoid">{KIND_LABEL.avoid}</option>
				<option value="defend">{KIND_LABEL.defend}</option>
			</select>
			<label class="add-text">
				<span class="sr-only">Rule</span>
				<Input
					bind:value={addText}
					placeholder={PLACEHOLDER[addKind]}
					autocomplete="off"
					disabled={!ready}
					onkeydown={addKeydown}
				/>
			</label>
			<Button variant="secondary" size="sm" disabled={!ready || !addText.trim()} onclick={add}>
				Add rule
			</Button>
		</div>
	{/if}

</section>

<style>
	.rules {
		display: flex;
		flex-direction: column;
		gap: 12px;
		/* The row stacks by the width it is given, not by the window's. */
		container-type: inline-size;
	}

	.list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.row {
		display: grid;
		grid-template-columns: 8rem minmax(0, 1fr) auto;
		align-items: center;
		column-gap: 12px;
		/* Tall enough for the Undo button, so removing a rule moves nothing. */
		min-height: 32px;
	}

	.kind,
	.removed {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.removed {
		grid-column: 1 / 3;
	}

	.text {
		font-size: 15px;
		color: var(--color-foreground);
		overflow-wrap: anywhere;
	}

	.edit {
		grid-column: 2 / -1;
	}

	.actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	/* Each control's touch halo is 44px; spaced this far apart, the two do
	   not overlap and a tap lands on the one under the thumb. */
	@media (pointer: coarse) {
		.actions {
			gap: 20px;
		}
	}

	@container (max-width: 479px) {
		.row {
			grid-template-columns: minmax(0, 1fr) auto;
		}

		.kind,
		.edit {
			grid-column: 1 / -1;
		}

		.removed {
			grid-column: 1;
		}
	}

	.add {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.add select {
		height: 40px;
		padding: 0 12px;
		font: inherit;
		font-size: 14px;
		color: var(--color-foreground);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
	}

	@media (pointer: coarse) {
		.add select {
			height: 44px;
		}
	}

	.add-text {
		flex: 1 1 12rem;
		min-width: 0;
	}

	.sec-hint,
	.field-error {
		margin: 0;
		max-width: 60ch;
		font-size: 12px;
	}

	.sec-hint {
		color: var(--color-foreground-subtle);
	}

	.field-error {
		color: var(--color-error);
	}
</style>
