<script lang="ts">
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

	let { assistantName = 'Ari' }: { assistantName?: string } = $props();

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
	<h2 class="settings-label">Spending on requests</h2>
	<p class="sec-hint">
		When you hand {assistantName} a task, it works on it in the background. These limits
		cap what that work can spend through Virtues.
	</p>
	<div class="fields">
		<Input
			label="Each run, in dollars"
			bind:value={run}
			disabled={!loaded}
			autoSave
			onSave={(v: string | number) => save('request_run_limit_micros', v)}
		/>
		<Input
			label="Each day, in dollars"
			bind:value={day}
			disabled={!loaded}
			autoSave
			onSave={(v: string | number) => save('request_day_limit_micros', v)}
		/>
	</div>
	{#if error}
		<p class="field-error" role="alert">{error}</p>
	{/if}
	<p class="sec-hint">
		A run that reaches its limit stops and shows you what it has so far. Once a day's limit is reached,
		requests wait until the day's spending falls under it. With your own AI key, the cost is unknown,
		so these limits do not apply.
	</p>
</section>

<style>
	section {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.fields {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 12rem));
		gap: 16px;
	}

	@media (max-width: 480px) {
		.fields {
			grid-template-columns: minmax(0, 1fr);
		}
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
