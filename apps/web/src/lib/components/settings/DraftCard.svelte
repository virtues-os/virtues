<script lang="ts">
	/**
	 * What the assistant would change, from what was typed in the Settings
	 * search field (Return). Each change shows its setting, the value now and
	 * the value drafted; nothing is saved until Apply. Applied changes are
	 * recorded as the assistant's and can be undone from Changed lately.
	 *
	 * The server drafts only text and money settings and checks every value
	 * (`api/settings_changes.rs`); when nothing it can change matches, the
	 * card offers to ask in a chat instead.
	 */
	import { onMount } from 'svelte';
	import Button from '$lib/components/Button.svelte';
	import { applySettingsDraft, draftSettings, ApiError, type SettingsDraft } from '$lib/api/client';
	import { askVirtues } from '$lib/stores/pendingPrompt.svelte';
	import { findSetting, titleOf } from '$lib/settings';
	import { addedText, displayValue } from '$lib/settings/changes';

	let {
		text,
		assistantName,
		onapplied,
		ondismiss,
		onedit,
	}: {
		text: string;
		assistantName: string;
		onapplied: () => void;
		ondismiss: () => void;
		/** Edit first: go to the setting's row. */
		onedit: (settingId: string) => void;
	} = $props();

	let draft = $state<SettingsDraft | null>(null);
	let error = $state<string | null>(null);
	let applying = $state(false);

	async function load() {
		error = null;
		draft = null;
		try {
			draft = await draftSettings(text);
		} catch (e) {
			error = e instanceof ApiError ? e.message : `${assistantName} couldn't draft that. Try again.`;
		}
	}

	onMount(load);

	async function apply() {
		if (!draft) return;
		applying = true;
		error = null;
		try {
			// The old values let the server refuse a draft that went stale.
			await applySettingsDraft(draft.changes.map((c) => ({ field: c.field, new_value: c.new_value, old_value: c.old_value })));
			onapplied();
		} catch (e) {
			error = e instanceof ApiError ? e.message : "Your server couldn't apply that. Nothing was changed.";
		} finally {
			applying = false;
		}
	}

	const label = (settingId: string) => titleOf(findSetting(settingId)?.title ?? settingId, assistantName);
</script>

<section class="card" aria-label="Drafted changes" aria-busy={!draft && !error}>
	{#if !draft && !error}
		<p class="status">{assistantName} is drafting “{text}”…</p>
	{:else if error}
		<p class="status error">{error}</p>
		<div class="actions">
			<Button variant="secondary" onclick={load}>Try again</Button>
			<Button variant="ghost" onclick={ondismiss}>Close</Button>
		</div>
	{:else if draft && draft.changes.length === 0}
		<h2>{assistantName} didn't find a setting for that</h2>
		{#if draft.note}<p class="note">{draft.note}</p>{/if}
		<div class="actions">
			<Button variant="secondary" onclick={() => askVirtues(text)}>Ask in a chat</Button>
			<Button variant="ghost" onclick={ondismiss}>Close</Button>
		</div>
	{:else if draft}
		<h2>{assistantName} drafted {draft.changes.length === 1 ? 'a change' : 'these changes'}</h2>
		<ul>
			{#each draft.changes as change (change.field)}
				{@const added = addedText(change.old_value, change.new_value)}
				<li>
					<span class="which">{label(change.setting_id)}</span>
					{#if added}
						<p class="text">{added.kept}<mark>{added.added}</mark></p>
					{:else if typeof change.new_value === 'string' && change.new_value.length > 40}
						<p class="text"><mark>{change.new_value}</mark></p>
					{:else}
						<p class="swap">
							<span class="was">{displayValue(change.setting_id, change.old_value)}</span>
							<span aria-hidden="true">→</span>
							<span class="now">{displayValue(change.setting_id, change.new_value)}</span>
						</p>
					{/if}
				</li>
			{/each}
		</ul>
		<div class="actions">
			<Button variant="primary" loading={applying} onclick={apply}>
				{draft.changes.length === 1 ? 'Apply' : 'Apply all'}
			</Button>
			<Button variant="secondary" onclick={() => onedit(draft!.changes[0].setting_id)}>Edit first</Button>
			<Button variant="ghost" onclick={ondismiss}>Not this</Button>
			<span class="aside">Nothing changes until you apply. You can undo it in Changed lately.</span>
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
		font-size: 1.25rem;
		font-weight: 400;
	}

	.status {
		margin: 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.status.error {
		color: var(--color-error);
		margin-bottom: 12px;
	}

	.note {
		margin: 0 0 16px;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	ul {
		list-style: none;
		margin: 0 0 16px;
		padding: 0;
	}

	li {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr);
		gap: 16px;
		padding: 12px 0;
		border-top: 1px solid var(--color-border-subtle);
	}

	.which {
		font-size: 14px;
		color: var(--color-foreground);
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

	.swap {
		display: flex;
		align-items: baseline;
		gap: 12px;
		margin: 0;
		font-size: 15px;
	}

	.was {
		color: var(--color-foreground-muted);
		text-decoration: line-through;
	}

	.now {
		color: var(--color-foreground);
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
