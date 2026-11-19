<script lang="ts">
	/**
	 * The settings agent's card for what was typed in the Settings search
	 * field (Return). The server says which card (`api/settings_draft.rs`):
	 *
	 * - change: the settings it would change, each before and after. Apply
	 *   saves them; Changed lately can undo them. A change that can't be put
	 *   back says what happens and its button names the action.
	 * - preview: a change to appearance only is put on at once, and Keep or
	 *   Put back decides. Leaving the card puts it back.
	 * - choose: two or three drafts for an unclear request; picking one makes
	 *   it the change.
	 * - open: something you do on a page (a key, a card, a device in your
	 *   hand), with why and a button there.
	 * - answer: a question about your settings, answered, with the latest
	 *   change to what it is about.
	 * - none: the settings that matched what you typed, to open.
	 *
	 * Nothing is saved until you apply. Appearance changes are applied here
	 * (`lib/settings/appearance`), everything else by the server.
	 */
	import { onDestroy, onMount } from 'svelte';
	import Button from '$lib/components/Button.svelte';
	import {
		applySettingsDraft,
		draftSettings,
		getSettingsChanges,
		undoSettingsChange,
		ApiError,
		type SettingsChange,
		type SettingsDraft,
		type SettingsDraftChange,
	} from '$lib/api/client';
	import { findSetting, settingPlace, titleOf, type Setting } from '$lib/settings';
	import { appearanceNow, applyAppearance } from '$lib/settings/appearance';
	import { changeRowId, changeSentence, displayValue, mergeChanges, textDiff, whenAt } from '$lib/settings/changes';

	let {
		text,
		assistantName,
		matches,
		onapplied,
		ondismiss,
		onopen,
	}: {
		text: string;
		assistantName: string;
		/** The settings that matched what was typed, for when the agent has nothing. */
		matches: Setting[];
		onapplied: () => void;
		ondismiss: () => void;
		/** Open a setting on its page. */
		onopen: (settingId: string) => void;
	} = $props();

	let draft = $state<SettingsDraft | null>(null);
	let error = $state<string | null>(null);
	let applying = $state(false);
	/** The option picked from a choose card. */
	let picked = $state<SettingsDraftChange | null>(null);
	/** Fields taken out of a change with their ×. */
	let leftOut = $state<string[]>([]);
	/** For an answer: the latest change to each setting it is about. */
	let recent = $state<SettingsChange[]>([]);
	/** Appearance changes on screen as a preview, until kept or put back. */
	let previewing: SettingsDraftChange[] = [];
	let kept = false;

	const changes = $derived.by(() => {
		const all = draft?.kind === 'change' ? draft.changes : picked ? [picked] : [];
		return all.filter((c) => !leftOut.includes(c.field));
	});
	const preview = $derived(changes.length > 0 && changes.every((c) => c.in_app));
	const lasting = $derived(changes.filter((c) => !c.can_undo));

	const title = (settingId: string) => titleOf(findSetting(settingId)?.title ?? settingId, assistantName);
	const placeOf = (settingId: string) => {
		const setting = findSetting(settingId);
		return setting ? settingPlace(setting, assistantName) : '';
	};

	async function load() {
		error = null;
		draft = null;
		picked = null;
		leftOut = [];
		try {
			draft = await draftSettings(text, appearanceNow());
		} catch (e) {
			error = e instanceof ApiError ? e.message : `${assistantName} couldn't read that. Try again.`;
			return;
		}
		if (draft.kind === 'change') startPreview(draft.changes);
		if (draft.kind === 'answer' && draft.about.length) void loadRecent(draft.about);
	}

	onMount(load);
	onDestroy(putBack);

	/** Put appearance changes on at once, when that is all a change is. */
	function startPreview(list: SettingsDraftChange[]) {
		if (!list.length || !list.every((c) => c.in_app)) return;
		previewing = list;
		for (const c of list) void applyAppearance(c.field, c.new_value, false);
	}

	function putBack() {
		if (kept) return;
		for (const c of previewing) void applyAppearance(c.field, c.old_value, false);
		previewing = [];
	}

	function pick(option: SettingsDraftChange) {
		picked = option;
		startPreview([option]);
	}

	function leaveOut(change: SettingsDraftChange) {
		leftOut = [...leftOut, change.field];
		const shown = previewing.find((c) => c.field === change.field);
		if (shown) {
			void applyAppearance(shown.field, shown.old_value, false);
			previewing = previewing.filter((c) => c !== shown);
		}
	}

	function dismiss() {
		putBack();
		ondismiss();
	}

	async function apply() {
		applying = true;
		error = null;
		try {
			const server = changes.filter((c) => !c.in_app);
			// The old values let the server refuse a draft that went stale.
			if (server.length) await applySettingsDraft(server.map((c) => ({ field: c.field, new_value: c.new_value, old_value: c.old_value })));
			for (const c of changes.filter((c) => c.in_app)) await applyAppearance(c.field, c.new_value, true);
			kept = true;
			onapplied();
		} catch (e) {
			error = e instanceof ApiError ? e.message : "Your server couldn't apply that. Nothing was changed.";
		} finally {
			applying = false;
		}
	}

	async function loadRecent(about: string[]) {
		try {
			const all = mergeChanges((await getSettingsChanges()).changes);
			recent = about.flatMap((id) => all.find((c) => c.setting_id === id) ?? []).slice(0, 3);
		} catch {
			// The answer stands without them.
			recent = [];
		}
	}

	async function undo(change: SettingsChange) {
		const id = changeRowId(change);
		if (id === null) return;
		try {
			await undoSettingsChange(id);
			onapplied();
		} catch (e) {
			error = e instanceof ApiError ? e.message : "Your server couldn't undo that change. Try again.";
		}
	}

	/** The button for a change that can't be put back names what it does. */
	function lastingLabel(list: SettingsDraftChange[]): string {
		return list.some((c) => c.setting_id === 'keep-recordings') ? 'Delete older audio' : 'Apply anyway';
	}

	function applyLabel(count: number): string {
		return count === 1 ? 'Apply' : count === 2 ? 'Apply both' : 'Apply all';
	}

	/** Nothing was set, so a value reads as added rather than as a swap.
	 *  Recordings kept forever are null and still read as a swap. */
	function isEmpty(value: unknown): boolean {
		return value === null || value === undefined || value === '';
	}

	/** Long text reads as a diff; anything else as before → after. */
	function asText(change: SettingsDraftChange): boolean {
		return typeof change.new_value === 'string' && (change.setting_id === 'style' || change.new_value.length > 40);
	}
</script>

<section class="card" aria-label="{assistantName}'s answer" aria-busy={!draft && !error}>
	{#if !draft && !error}
		<p class="status">{assistantName} is reading “{text}”…</p>
	{:else if error && !draft}
		<p class="status error">{error}</p>
		<div class="actions">
			<Button variant="secondary" onclick={load}>Try again</Button>
			<Button variant="secondary" onclick={dismiss}>Close</Button>
		</div>
	{:else if draft && changes.length && preview}
		<h2>{changes.length === 1 ? `This is ${displayValue(changes[0].setting_id, changes[0].new_value)}` : "Here's how it looks"}</h2>
		<ul>
			{#each changes as change (change.field)}
				<li>
					<button type="button" class="which" onclick={() => onopen(change.setting_id)}>{title(change.setting_id)}</button>
					<p class="swap">
						<span class="was">{displayValue(change.setting_id, change.old_value)}</span>
						<span class="arrow" aria-hidden="true">→</span>
						<span>{displayValue(change.setting_id, change.new_value)}</span>
					</p>
					<span></span>
				</li>
			{/each}
		</ul>
		{#if error}<p class="status error">{error}</p>{/if}
		<div class="actions">
			<Button variant="primary" loading={applying} onclick={apply}>Keep</Button>
			<Button variant="secondary" onclick={dismiss}>Put back</Button>
			<span class="aside">Goes back if you leave.</span>
		</div>
	{:else if draft && changes.length}
		<h2>{assistantName} drafted {changes.length === 1 ? 'a change' : 'these changes'}</h2>
		<ul>
			{#each changes as change (change.field)}
				{@const parts = asText(change) ? textDiff(change.old_value, change.new_value) : null}
				<li>
					<button type="button" class="which" onclick={() => onopen(change.setting_id)}>{title(change.setting_id)}</button>
					{#if parts}
						<p class="text">
							{#each parts as part, i (i)}{#if part.kind === 'added'}<mark>{part.text}</mark>{:else if part.kind === 'removed'}<del>{part.text}</del>{:else}{part.text}{/if}{' '}{/each}
						</p>
					{:else if isEmpty(change.old_value)}
						<!-- A first value: nothing to strike out. -->
						<p class="swap"><span>{displayValue(change.setting_id, change.new_value)}</span></p>
					{:else}
						<p class="swap">
							<span class="was">{displayValue(change.setting_id, change.old_value)}</span>
							<span class="arrow" aria-hidden="true">→</span>
							<span>{displayValue(change.setting_id, change.new_value)}</span>
						</p>
					{/if}
					{#if changes.length > 1}
						<button type="button" class="out" aria-label="Leave {title(change.setting_id)} out" onclick={() => leaveOut(change)}>
							<svg viewBox="0 0 24 24" width="14" height="14" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" /></svg>
						</button>
					{:else}
						<span></span>
					{/if}
				</li>
			{/each}
		</ul>
		{#each changes.filter((c) => c.consequence) as change (change.field)}
			<p class="consequence">{change.consequence}</p>
		{/each}
		{#if error}<p class="status error">{error}</p>{/if}
		<div class="actions">
			{#if lasting.length}
				<Button variant="danger" loading={applying} onclick={apply}>{lastingLabel(lasting)}</Button>
			{:else}
				<Button variant="primary" loading={applying} onclick={apply}>{applyLabel(changes.length)}</Button>
			{/if}
			<Button variant="secondary" onclick={dismiss}>Not this</Button>
		</div>
	{:else if draft?.kind === 'choose'}
		<h2>Which of these?</h2>
		<div class="rows">
			{#each draft.options as option (option.field + JSON.stringify(option.new_value))}
				<button type="button" class="row" onclick={() => pick(option)}>
					<span class="row-title">{title(option.setting_id)}</span>
					<span class="row-value"><span class="was-plain">{displayValue(option.setting_id, option.old_value)}</span> → {displayValue(option.setting_id, option.new_value)}</span>
					<svg class="chev" viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" /></svg>
				</button>
			{/each}
		</div>
		<div class="actions">
			<Button variant="secondary" onclick={dismiss}>Not this</Button>
		</div>
	{:else if draft?.kind === 'open' && draft.open}
		<h2>You do this on its page</h2>
		<p class="note">{draft.open.reason.replaceAll('{name}', assistantName)}</p>
		<div class="rows">
			<button type="button" class="row lit" onclick={() => onopen(draft!.open!.setting_id)}>
				<span class="row-title">{title(draft.open.setting_id)}</span>
				<span class="row-value">{placeOf(draft.open.setting_id)}</span>
				<svg class="chev" viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" /></svg>
			</button>
		</div>
		<div class="actions">
			<Button variant="secondary" onclick={dismiss}>Close</Button>
		</div>
	{:else if draft?.kind === 'answer' && draft.answer}
		<p class="answer">{draft.answer}</p>
		{#if recent.length}
			<ul class="recent">
				{#each recent as change (change.id)}
					<li>
						<span class="line">
							<button type="button" class="which" onclick={() => change.setting_id && onopen(change.setting_id)}>{title(change.setting_id ?? '')}</button>
							<span class="sub">{changeSentence(change, assistantName)}</span>
							<span class="when">· {whenAt(change.occurred_at)}</span>
						</span>
						{#if change.can_undo}
							<button type="button" class="link" onclick={() => undo(change)}>Undo</button>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
		{#if error}<p class="status error">{error}</p>{/if}
		<div class="actions">
			<Button variant="secondary" onclick={dismiss}>Close</Button>
		</div>
	{:else if draft}
		<h2>{assistantName} can't change that from here</h2>
		{#if draft.note}<p class="note">{draft.note}</p>{/if}
		{#if matches.length}
			<div class="rows">
				{#each matches.slice(0, 3) as setting, i (setting.id)}
					<button type="button" class="row" class:lit={i === 0} onclick={() => onopen(setting.id)}>
						<span class="row-title">{titleOf(setting.title, assistantName)}</span>
						<span class="row-value">{settingPlace(setting, assistantName)}</span>
						<svg class="chev" viewBox="0 0 24 24" width="15" height="15" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" /></svg>
					</button>
				{/each}
			</div>
		{/if}
		<div class="actions">
			<Button variant="secondary" onclick={dismiss}>Close</Button>
		</div>
	{/if}
</section>

<style>
	.card {
		margin-top: 24px;
		padding: 20px 24px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
	}

	h2 {
		margin: 0 0 12px;
		font-size: 1.375rem;
		font-weight: 400;
	}

	.status {
		margin: 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.status.error {
		color: var(--color-error);
		margin: 0 0 12px;
	}

	.note {
		margin: -4px 0 14px;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	ul {
		list-style: none;
		margin: 0 0 12px;
		padding: 0;
	}

	li {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr) 28px;
		align-items: baseline;
		gap: 16px;
		padding: 12px 0;
		border-top: 1px solid var(--color-border-subtle);
	}

	.which {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 14px;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.which:hover {
		text-decoration: underline;
	}

	.text {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 17px;
		line-height: 1.55;
		color: var(--color-foreground);
	}

	mark {
		background: var(--color-highlight);
		color: inherit;
		border-radius: var(--radius-sm);
	}

	del {
		color: var(--color-foreground-subtle);
	}

	.swap {
		display: flex;
		align-items: baseline;
		gap: 12px;
		margin: 0;
		font-size: 15px;
	}

	.was {
		color: var(--color-foreground-subtle);
		text-decoration: line-through;
	}

	.arrow,
	.was-plain {
		color: var(--color-foreground-subtle);
	}

	.out {
		align-self: center;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		padding: 0;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.out:hover {
		background: var(--color-surface-elevated);
		color: var(--color-foreground);
	}

	.consequence {
		margin: 0 0 14px;
		padding: 10px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		font-size: 14px;
		line-height: 1.5;
	}

	.rows {
		margin: 0 0 12px;
	}

	.row {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr) auto;
		align-items: center;
		gap: 16px;
		width: calc(100% + 24px);
		min-height: 48px;
		margin: 0 -12px;
		padding: 0 12px;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		font: inherit;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.row:hover,
	.row.lit {
		background: var(--color-surface-elevated);
	}

	.row-title {
		font-size: 15px;
	}

	.row-value {
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.chev {
		color: var(--color-foreground-muted);
	}

	.answer {
		margin: 0 0 14px;
		font-family: var(--font-serif);
		font-size: 19px;
		line-height: 1.45;
	}

	.recent li {
		grid-template-columns: minmax(0, 1fr) auto;
		padding: 10px 0;
	}

	.line {
		font-size: 14px;
	}

	.sub {
		color: var(--color-foreground-muted);
	}

	.when {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	.link {
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		font-size: 13px;
		color: var(--color-primary);
		cursor: pointer;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}

	.aside {
		margin-left: auto;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
</style>
