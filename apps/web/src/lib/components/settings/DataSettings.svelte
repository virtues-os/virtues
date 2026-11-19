<script lang="ts">
	/**
	 * Settings · Your data: how long audio is kept (`keep_recordings_days`,
	 * enforced by the maintenance sweeper, `api/recordings.rs`), and the
	 * command that exports everything (`virtues export`, `cli/export.rs`).
	 */
	import { onMount } from 'svelte';
	import SettingRow from './SettingRow.svelte';
	import Select from '$lib/components/Select.svelte';
	import { getProfile, updateProfile } from '$lib/api/client';

	type Keep = 'forever' | '365' | '90' | '30';

	const KEEP = [
		{ value: 'forever', label: 'Forever' },
		{ value: '365', label: '1 year' },
		{ value: '90', label: '90 days' },
		{ value: '30', label: '30 days' },
	] as const satisfies readonly { value: Keep; label: string }[];

	let keep = $state<Keep>('forever');
	let phase = $state<'loading' | 'ready' | 'error'>('loading');
	let saveError = $state<string | null>(null);

	async function load() {
		phase = 'loading';
		try {
			const days = (await getProfile()).keep_recordings_days;
			keep = days ? (String(days) as Keep) : 'forever';
			phase = 'ready';
		} catch {
			phase = 'error';
		}
	}

	onMount(load);

	async function chooseKeep(next: Keep) {
		const was = keep;
		keep = next;
		saveError = null;
		try {
			await updateProfile({ keep_recordings_days: next === 'forever' ? null : Number(next) });
		} catch {
			keep = was;
			saveError = "Your server couldn't save this. Nothing was deleted. Try again.";
		}
	}

	const keepHelp = $derived(
		saveError ??
			(keep === 'forever'
				? 'Your server keeps the audio of every recording.'
				: 'Audio older than this is deleted once it is transcribed. Transcripts stay, and copies already on a backup drive stay there.'),
	);
</script>

{#if phase === 'error'}
	<div class="load-error" role="alert">
		<p>Your server couldn't read how long it keeps recordings.</p>
		<button type="button" class="again" onclick={load}>Try again</button>
	</div>
{:else}
	<SettingRow id="keep-recordings" label="Keep recordings" help={keepHelp} forId="keep-recordings-choice">
		<Select id="keep-recordings-choice" value={keep} options={KEEP} disabled={phase !== 'ready'} onchange={chooseKeep} />
	</SettingRow>
{/if}

<SettingRow id="export" label="Export everything" help="Your records, pages and files as readable files, run on your server. Keys and passwords stay out, and the copy isn't encrypted." block>
	<code class="command">sudo virtues export</code>
	<p class="note">
		It writes to <code>/var/lib/virtues/exports</code>, or to a folder you name after the command. Add
		<code>--skip-files</code> to leave out recordings and photos.
	</p>
</SettingRow>

<style>
	code {
		font-family: var(--font-mono);
		font-size: 12px;
		white-space: nowrap;
	}

	code.command {
		display: block;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		font-family: var(--font-mono);
		font-size: 13px;
	}

	.note {
		margin: 8px 0 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.load-error {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 0;
		font-size: 13px;
		color: var(--color-error);
	}

	.load-error p {
		margin: 0;
	}

	.again {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
</style>
