<script lang="ts">
	/**
	 * Settings · Your server · Backup (/virtues/backup): how old the newest
	 * backup is, each drive the server backs up to, and the commands that set
	 * it up and bring it back (`GET /api/backup/status`,
	 * docs/operate/backup-and-restore.md). Status and Your data link here.
	 *
	 * The page leads with one sentence for the state the server reports:
	 * - none: no drive is registered, so the server holds the only copy. The
	 *   setup steps are the page.
	 * - never: a drive is registered and no backup has finished yet.
	 * - failing: a drive's last run stopped with an error. The error, as the
	 *   backup run stored it, sits under that drive.
	 * - stale, ok: when the newest backup finished.
	 *
	 * Setting up and restoring are commands on the server, with no buttons:
	 * creating the recovery key has to happen where someone reads the secret
	 * it prints, a drive has to be mounted first, and restoring needs the
	 * server stopped, so this page cannot do any of them for you.
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import Command from './Command.svelte';
	import Fold from './Fold.svelte';
	import LoadError from './LoadError.svelte';
	import { getBackupStatus, type BackupStatus, type BackupVolumeStatus } from '$lib/api/client';
	import { backupAgo } from '$lib/settings/backup';

	const GUIDE = 'https://virtues.com/docs/operate/backup-and-restore';
	const ADD_DRIVE = 'sudo virtues volumes add /path/to/drive --name "study drive"';
	const RESTORE = 'sudo virtues restore --from-volume /path/to/drive --key-file /path/to/key';

	let status = $state<BackupStatus | null>(null);
	let failed = $state(false);

	async function load() {
		failed = false;
		try {
			status = await getBackupStatus();
		} catch {
			failed = true;
		}
	}

	onMount(load);

	/** "Study drive", "Study drive and Office drive", "A, B and C". */
	function names(list: BackupVolumeStatus[]): string {
		const n = list.map((v) => v.name);
		return n.length <= 2 ? n.join(' and ') : `${n.slice(0, -1).join(', ')} and ${n[n.length - 1]}`;
	}

	const failing = $derived(status?.volumes.filter((v) => v.last_error) ?? []);

	const lead = $derived.by(() => {
		if (!status) return '';
		switch (status.state) {
			case 'none':
				return 'Your server holds the only copy of your data.';
			case 'never':
				return "Your server hasn't finished a backup to a drive yet.";
			case 'failing':
				return "Your server couldn't finish its last backup.";
			default:
				return `Your last backup finished ${backupAgo(status.age_seconds)}.`;
		}
	});

	const sub = $derived.by(() => {
		if (!status) return '';
		switch (status.state) {
			case 'none':
				return 'A backup drive fixes that. Setting one up takes three steps on your server.';
			case 'failing':
				return `The backup to ${names(failing)} stopped with the error below. Your server tries again tonight at 04:00.`;
			case 'stale':
				return 'Plug in a backup drive and the next nightly run brings it up to date.';
			default:
				return 'Every night at 04:00 your server backs up to each of your drives that is plugged in. Only your recovery key opens a backup.';
		}
	});

	function driveLine(v: BackupVolumeStatus): string {
		const where = v.attached ? 'Plugged in.' : 'Not plugged in.';
		if (!v.last_ok_at) return `${where} No backup yet.`;
		return `${where} Last ${v.last_error ? 'good ' : ''}backup ${backupAgo(v.age_seconds)}.`;
	}
</script>

<SettingsPage title="Backup" lede="An encrypted copy of your data, on drives you keep." back={{ route: '/virtues/system', label: 'Your server' }}>
	{#if failed}
		<LoadError message="Your server couldn't read its backup status." onretry={load} />
	{:else if status}
		<p class="settings-lead">{lead}</p>
		<p class="settings-sub">{sub}</p>

		{#if status.state === 'none'}
			<section class="group">
				<h2 class="settings-label">Setting it up</h2>
				<ol class="steps">
					<li>
						<span class="n" aria-hidden="true">1</span>
						<div>
							<p>
								Create your recovery key, once. Keep what it prints somewhere other than your server. Without it, nothing can
								open a backup.
							</p>
							<Command command="sudo virtues backup --init-key" />
						</div>
					</li>
					<li>
						<span class="n" aria-hidden="true">2</span>
						<div>
							<p>Plug in a drive, mount it, and register it.</p>
							<Command command={ADD_DRIVE} />
						</div>
					</li>
					<li>
						<span class="n" aria-hidden="true">3</span>
						<div>
							<p>That's all. Every night at 04:00 your server backs up to each registered drive that is plugged in.</p>
						</div>
					</li>
				</ol>
			</section>
		{:else}
			<section class="group">
				<h2 class="settings-label">Drives</h2>
				{#each status.volumes as volume (volume.id)}
					<div class="drive">
						<span class="name">{volume.name}</span>
						<div class="state">
							{driveLine(volume)}
							{#if volume.last_error}<code class="error">{volume.last_error}</code>{/if}
						</div>
					</div>
				{/each}
				<Fold label="Add another drive" sub="Plug it in and mount it, then register it with one command on your server.">
					<Command command={ADD_DRIVE} />
				</Fold>
			</section>
		{/if}

		<section class="group">
			<h2 class="settings-label">Bringing it back</h2>
			<Fold label="Restore on a new or rebuilt server" sub="With the drive mounted and your recovery key saved to a file.">
				<Command command={RESTORE} />
			</Fold>
		</section>

		<p class="settings-aside">
			The manual's <a href={GUIDE} target="_blank" rel="noopener noreferrer">Backup and restore</a> page explains what a backup
			holds and how to check that one opens.
		</p>
	{/if}
</SettingsPage>

<style>
	.group {
		padding: 24px 0 8px;
	}

	.drive {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr);
		align-items: start;
		gap: 16px;
		min-height: 52px;
		padding-top: 16px;
	}

	.name {
		font-size: 15px;
		color: var(--color-foreground);
	}

	.state {
		font-size: 15px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.error {
		display: block;
		margin-top: 6px;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.45;
		color: var(--color-error);
		overflow-wrap: anywhere;
	}

	.steps {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.steps li {
		display: grid;
		grid-template-columns: 28px minmax(0, 1fr);
		gap: 8px;
		padding: 12px 0;
	}

	.n {
		font-family: var(--font-serif);
		font-size: 20px;
		line-height: 1.2;
		color: var(--color-foreground-muted);
	}

	.steps p {
		margin: 0 0 8px;
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground);
	}

	@media (max-width: 560px) {
		.drive {
			grid-template-columns: minmax(0, 1fr);
			gap: 4px;
		}
	}
</style>
