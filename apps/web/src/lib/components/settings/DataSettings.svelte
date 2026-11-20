<script lang="ts">
	/**
	 * Settings · Your data: copies of your data, what your server keeps, and
	 * what you've shared.
	 *
	 * - Backup opens its page; its row says how old the newest backup is
	 *   (`GET /api/backup/status`).
	 * - Export everything is a command, because `virtues export` (`cli/export.rs`)
	 *   writes on the server and the server has no export endpoint.
	 * - Keep recordings is `keep_recordings_days`, enforced by the maintenance
	 *   sweeper (`api/recordings.rs`).
	 * - Recently deleted holds chats, pages, threads and Drive files for 30 days.
	 * - A record (a purchase, a visit) is deleted through the assistant's
	 *   `delete_data` tool or `/delete-data` (`api/deletions.rs`). It is gone at
	 *   once and tombstoned, so it never passes through Recently deleted; the
	 *   row says so because the two kinds of deleting are easy to confuse.
	 * - Shared links and Keys open their own pages under this one.
	 */
	import { onMount } from 'svelte';
	import SettingRow from './SettingRow.svelte';
	import LinkRow from './LinkRow.svelte';
	import Command from './Command.svelte';
	import Segmented from '$lib/components/Segmented.svelte';
	import LoadError from './LoadError.svelte';
	import { getBackupStatus, getProfile, updateProfile, type BackupStatus } from '$lib/api/client';
	import { backupSummary } from '$lib/settings/backup';


	const MANUAL = 'https://virtues.com/docs#what-leaves-the-server';

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
	let backup = $state<BackupStatus | null>(null);

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

	onMount(() => {
		void load();
		// absent-ok: without a status the row reads "Open", and the Backup page
		// says what went wrong.
		getBackupStatus()
			.then((s) => (backup = s))
			.catch(() => (backup = null));
	});

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
			'Your server keeps the audio of every recording. A shorter time deletes older audio once it is transcribed. Transcripts stay, and copies already on a backup drive stay there.',
	);
</script>

<section class="group">
	<h2 class="settings-label">A copy of everything</h2>
	<LinkRow
		id="backup"
		label="Backup"
		help="Encrypted, every night, to the drives you plug into your server."
		href="/virtues/backup"
		value={backup ? backupSummary(backup) : undefined}
	/>
	<SettingRow id="export" label="Export everything">
		<div class="control">
			<Command command="sudo virtues export" />
			<p class="note">
				Your records, pages and files as readable files, written to <code>/var/lib/virtues/exports</code> on your server,
				or to a folder you name after the command. Keys and passwords stay out, and the copy isn't encrypted. Add
				<code>--skip-files</code> to leave out recordings and photos.
			</p>
		</div>
	</SettingRow>
</section>

<section class="group">
	<h2 class="settings-label">What your server keeps</h2>
	{#if phase === 'error'}
		<LoadError message="Your server couldn't read how long it keeps recordings." onretry={load} />
	{:else}
		<SettingRow id="keep-recordings" label="Keep recordings" help={keepHelp}>
			<Segmented id="keep-recordings-choice" label="Keep recordings" value={keep} options={KEEP} disabled={phase !== 'ready'} onchange={chooseKeep} />
		</SettingRow>
	{/if}
	<LinkRow
		id="recently-deleted"
		label="Recently deleted"
		help="Chats, pages, threads and files you've deleted. Each waits 30 days, and you can bring it back until then."
		href="/storage/trash"
	/>
	<LinkRow
		id="deleted-records"
		label="Deleted records"
		help="Records you deleted for good, such as a purchase or a visit. No sync brings them back."
		href="/virtues/data/deleted"
	/>
</section>

<section class="group">
	<h2 class="settings-label">Shared links and keys</h2>
	<LinkRow
		id="shared-links"
		label="Shared links"
		help="Every page, chat or applet you've shared. Turn a link off and it stops opening at once."
		href="/virtues/data/shared"
	/>
	<LinkRow id="keys" label="Keys" help="What your applets sign in with, such as an API key." href="/virtues/data/keys" />
</section>

<p class="settings-aside">
	The manual lists <a href={MANUAL} target="_blank" rel="noopener noreferrer">what leaves your server</a>, and when.
</p>

<style>
	.group {
		padding: 24px 0 8px;
	}

	.control {
		width: 100%;
		min-width: 0;
	}

	.note {
		margin: 6px 0 0;
		font-size: var(--text-caption);
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	code {
		font-family: var(--font-mono);
		font-size: 0.92em;
		white-space: nowrap;
	}

</style>
