<script lang="ts">
	/**
	 * Settings → Backup (/virtues/backup): how old the newest backup is, each
	 * drive the server backs up to, and the commands that set it up and bring
	 * it back (`GET /api/backup/status`, docs/operate/backup-and-restore.md).
	 *
	 * This is the one place backup's state appears; System links here.
	 * Setting up and restoring are commands on the server, with no buttons:
	 * creating the recovery key has to happen where someone reads the secret
	 * it prints, a drive has to be mounted first, and restoring needs the
	 * server stopped, so this page cannot do any of them for you.
	 */
	import { onMount } from 'svelte';
	import SettingsPage from './SettingsPage.svelte';
	import Button from '$lib/components/Button.svelte';
	import { getBackupStatus, type BackupStatus } from '$lib/api/client';

	const GUIDE = 'https://virtues.com/docs/operate/backup-and-restore';

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

	/** "4 hours ago", "9 days ago". */
	function ago(secs: number | null): string {
		if (secs === null) return 'never';
		const h = Math.floor(secs / 3600);
		if (h < 1) return 'under an hour ago';
		if (h < 48) return `${h} hour${h === 1 ? '' : 's'} ago`;
		return `${Math.floor(h / 24)} days ago`;
	}

	const headline = $derived.by(() => {
		if (!status) return '';
		switch (status.state) {
			case 'none':
				return 'Your server holds the only copy of your data.';
			case 'never':
				return "Your server hasn't finished a backup to a drive yet.";
			case 'failing':
				return "Your server couldn't finish its last backup.";
			case 'stale':
				return `Your last backup finished ${ago(status.age_seconds)}.`;
			default:
				return `Your last backup finished ${ago(status.age_seconds)}.`;
		}
	});
</script>

<SettingsPage title="Backup">
	{#if failed}
		<div class="note">
			<p>Your server couldn't read its backup status.</p>
			<Button variant="secondary" onclick={load}>Try again</Button>
		</div>
	{:else if status}
		<p class="headline">{headline}</p>
		{#if status.state === 'stale'}
			<p class="sub">Plug in a backup drive and the next nightly run brings it up to date.</p>
		{/if}

		{#if status.volumes.length}
			<h2 class="label">Drives</h2>
			<ul class="drives">
				{#each status.volumes as volume (volume.id)}
					<li>
						<span class="what">
							<span class="name">{volume.name}</span>
							<span class="sub">
								{volume.attached ? 'Plugged in' : 'Not plugged in'}{volume.last_ok_at ? `, last backup ${ago(volume.age_seconds)}` : ', no backup yet'}
							</span>
							{#if volume.last_error}<span class="error">{volume.last_error}</span>{/if}
						</span>
					</li>
				{/each}
			</ul>
		{/if}

		<h2 class="label">Setting it up</h2>
		<ol class="steps">
			<li>
				<p>Create your recovery key, once. Keep what it prints somewhere other than the server: without it, nothing can open a backup.</p>
				<code>sudo virtues backup --init-key</code>
			</li>
			<li>
				<p>Plug in a drive, mount it, and register it.</p>
				<code>sudo virtues volumes add /path/to/drive --name "study drive"</code>
			</li>
			<li>
				<p>That's all. Every night at 04:00 your server backs up to each registered drive that is plugged in.</p>
			</li>
		</ol>

		<h2 class="label">Bringing it back</h2>
		<p class="body">On a new or rebuilt server, with the drive mounted and your recovery key saved to a file:</p>
		<code>sudo virtues restore --from-volume /path/to/drive --key-file /path/to/key</code>

		<p class="body guide">
			The manual's <a href={GUIDE} target="_blank" rel="noopener noreferrer">Backup and restore</a> page explains what a backup
			holds, how to check one opens, and restoring from a file.
		</p>
	{/if}
</SettingsPage>

<style>
	.headline {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 22px;
		line-height: 1.35;
		color: var(--color-foreground);
	}

	.sub {
		display: block;
		margin: 4px 0 0;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.label {
		margin: 32px 0 8px;
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 400;
		color: var(--color-foreground-muted);
	}

	.drives {
		list-style: none;
		margin: 0;
		padding: 0;
		border-top: 1px solid var(--color-border-subtle);
	}

	.drives li {
		padding: 12px 0;
		border-bottom: 1px solid var(--color-border-subtle);
	}

	.name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.error {
		display: block;
		margin-top: 4px;
		font-size: 13px;
		color: var(--color-error);
	}

	.steps {
		margin: 0;
		padding-left: 20px;
		list-style: decimal;
	}

	.steps li {
		margin-bottom: 16px;
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground);
	}

	.steps p,
	.body {
		margin: 0 0 8px;
		font-size: 15px;
		line-height: 1.5;
		color: var(--color-foreground);
	}

	code {
		display: block;
		padding: 8px 12px;
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		font-family: var(--font-mono);
		font-size: 13px;
		overflow-x: auto;
		white-space: nowrap;
	}

	.guide {
		margin-top: 24px;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.note {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
	}

	.note p {
		margin: 0;
		font-size: 14px;
		color: var(--color-error);
	}
</style>
