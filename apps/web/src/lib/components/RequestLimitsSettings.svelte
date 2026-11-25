<script lang="ts">
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	/**
	 * What the assistant may spend on requests it works in the background
	 * (Tasks): one limit for a single run and one for all runs in a day.
	 * The server stores them on the assistant profile in micros-USD and the
	 * request worker reads them before every run (`task_runner.rs`). Only
	 * spend through Virtues counts; your own key reports no cost.
	 */
	import { onMount } from 'svelte';
	// By path, not the `$lib` barrel, which the component tests cannot load.
	import Input from '$lib/components/Input.svelte';
	import { getAssistantProfile, updateAssistantProfile } from '$lib/api/client';

	let { assistantName = DEFAULT_ASSISTANT_NAME }: { assistantName?: string } = $props();

	type Key = 'request_run_limit_micros' | 'request_day_limit_micros';

	let run = $state('');
	let day = $state('');
	let loaded = $state(false);
	let error = $state<string | null>(null);

	const dollars = (micros: number) => (micros / 1_000_000).toFixed(2);

	onMount(async () => {
		try {
			const p = await getAssistantProfile<Record<Key, number>>();
			run = dollars(p.request_run_limit_micros);
			day = dollars(p.request_day_limit_micros);
			loaded = true;
		} catch {
			error = "Your server couldn't load these limits.";
		}
	});

	/** Save one limit, typed in dollars. Rejects so the field marks itself unsaved. */
	async function save(key: Key, raw: string | number) {
		const amount = Number(String(raw).replace(/^\$/, '').trim());
		if (!Number.isFinite(amount) || amount < 0) {
			error = 'Enter an amount in dollars, like 0.50.';
			throw new Error(error);
		}
		try {
			await updateAssistantProfile({ [key]: Math.round(amount * 1_000_000) });
			error = null;
		} catch (e) {
			error = "Your server couldn't save that limit. It kept the one before.";
			throw e;
		}
	}
</script>

<section>
	<h2 class="settings-label">Spending limits</h2>
	<p class="lead">Set the most {assistantName} may spend on tasks it runs in the background. Chats have no limit.</p>
	<div class="limit">
		<div>
			<label for="limit-run-field">One task</label>
			<p class="hint">If a task reaches this limit, {assistantName} stops it and shows you what it has so far.</p>
		</div>
		<div class="field"><span class="unit" aria-hidden="true">$</span><Input id="limit-run-field" bind:value={run} disabled={!loaded} autoSave onSave={(v: string | number) => save('request_run_limit_micros', v)} /></div>
	</div>
	<div class="limit">
		<div>
			<label for="limit-day-field">All tasks in a day</label>
			<p class="hint">If spending in the last 24 hours reaches this limit, new tasks wait until it drops below.</p>
		</div>
		<div class="field"><span class="unit" aria-hidden="true">$</span><Input id="limit-day-field" bind:value={day} disabled={!loaded} autoSave onSave={(v: string | number) => save('request_day_limit_micros', v)} /></div>
	</div>
	<p class="hint">Sites and repeating tasks have their own limits. These limits don't apply to calls to your own AI provider.</p>
	{#if error}
		<p class="field-error" role="alert">{error}</p>
	{/if}
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.limit {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 9rem;
		align-items: center;
		gap: 24px;
	}

	.limit label {
		font-size: 15px;
		color: var(--color-foreground);
	}

	.field {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.unit {
		font-size: 15px;
		color: var(--color-foreground-subtle);
	}

	.lead,
	.hint,
	.field-error {
		margin: 0;
		max-width: 60ch;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.lead {
		font-size: 15px;
	}

	.hint,
	.field-error {
		font-size: 13px;
	}

	.limit .hint {
		margin-top: 4px;
	}

	.field-error {
		color: var(--color-error);
	}
</style>
