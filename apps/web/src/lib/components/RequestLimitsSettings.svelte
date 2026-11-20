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
	<h2 class="settings-label">What tasks may spend</h2>
	<p class="sec-hint">
		For tasks you assign to {assistantName} to work on in the background. A task that reaches its
		limit stops and shows you what it has so far. When the last 24 hours reach the day's limit,
		tasks wait until spending drops below it. Chats and applets have no limit, and neither does
		anything that runs on your own AI provider.
	</p>
	<div class="limit">
		<label for="limit-run-field">One task</label>
		<div class="field"><span class="unit" aria-hidden="true">$</span><Input id="limit-run-field" bind:value={run} disabled={!loaded} autoSave onSave={(v: string | number) => save('request_run_limit_micros', v)} /></div>
	</div>
	<div class="limit">
		<label for="limit-day-field">All tasks in a day</label>
		<div class="field"><span class="unit" aria-hidden="true">$</span><Input id="limit-day-field" bind:value={day} disabled={!loaded} autoSave onSave={(v: string | number) => save('request_day_limit_micros', v)} /></div>
	</div>
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
