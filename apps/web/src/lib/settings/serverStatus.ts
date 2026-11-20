/**
 * The words on Settings · Your server: the one sentence it opens with, and
 * the short value each "What it runs" row ends in.
 *
 * The opening sentence names the first thing that needs you, in this order:
 * the database can't be reached, the disk is almost full, the last backup
 * failed, there is no recent backup, an applet needs you. With none of those
 * it says the server is running well. An update never takes it: your server
 * installs updates on its own, and the Updates row says when one is ready.
 */
import type { BackupStatus, SearchStatus, UpdateStatus } from '$lib/api/client';
import { backupAgo } from './backup';

/** A disk this full or fuller opens the page. */
export const DISK_FULL_PCT = 90;

/** Below this, the disk "has plenty of room". */
const DISK_ROOMY_PCT = 75;

export const RECOVERY_GUIDE = 'https://virtues.com/docs/operate/recovery';

/** What `/health` reports: `status` healthy or unhealthy, `database` connected or not. */
export interface Health {
	status: string;
	database: string;
}

export interface Disk {
	total: number;
	available: number;
}

export interface LeadInput {
	/** Null when `/health` didn't answer. */
	health: Health | null;
	/** The disk that holds your data. Null when telemetry didn't answer. */
	disk: Disk | null;
	/** Null when the backup status didn't answer. */
	backup: BackupStatus | null;
	/** Names of the applets that need you (`needsYou`). Null when the list didn't load. */
	appletsNeedingYou: string[] | null;
}

export interface Lead {
	kind: 'database' | 'disk' | 'backup-failing' | 'backup-none' | 'backup-never' | 'backup-stale' | 'applets' | 'well';
	sentence: string;
	sub: string;
	/** A Settings page that fixes it. */
	action?: { label: string; route: string };
	/** A manual page that explains what to check. */
	guide?: { label: string; href: string };
}

/** "20 GB", "500 GB", "1.2 TB". */
export function formatBytes(n: number): string {
	if (!n || n < 0) return '0 B';
	const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
	let i = 0;
	let v = n;
	while (v >= 1024 && i < units.length - 1) {
		v /= 1024;
		i++;
	}
	return `${v >= 10 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function diskUsedPct(disk: Disk): number {
	return disk.total ? Math.round(((disk.total - disk.available) / disk.total) * 100) : 0;
}

/** The disk that holds your data: `/`, else the largest, as the stored samples choose it. */
export function dataDisk<T extends Disk & { mount: string }>(disks: T[]): T | null {
	return disks.find((d) => d.mount === '/') ?? disks.reduce<T | null>((big, d) => (!big || d.total > big.total ? d : big), null);
}

export function serverLead(input: LeadInput): Lead {
	const { health, disk, backup, appletsNeedingYou } = input;

	if (health && (health.status !== 'healthy' || health.database !== 'connected')) {
		return {
			kind: 'database',
			sentence: 'Your server can’t reach its database.',
			sub: 'Your records can’t load or save until it can.',
			guide: { label: 'Recovery', href: RECOVERY_GUIDE },
		};
	}

	if (disk && diskUsedPct(disk) >= DISK_FULL_PCT) {
		return {
			kind: 'disk',
			sentence: 'Your server’s disk is almost full.',
			sub: `${formatBytes(disk.available)} of ${formatBytes(disk.total)} is left. A shorter time under Keep recordings deletes older audio once it is transcribed.`,
			action: { label: 'Open Your data', route: '/virtues/data' },
		};
	}

	if (backup) {
		const openBackup = { label: 'Open Backup', route: '/virtues/backup' };
		switch (backup.state) {
			case 'failing':
				return {
					kind: 'backup-failing',
					sentence: 'Your server couldn’t finish its last backup.',
					sub: 'The Backup page shows the error. Your server tries again tonight at 04:00.',
					action: openBackup,
				};
			case 'none':
				return {
					kind: 'backup-none',
					sentence: 'Your server holds the only copy of your data.',
					sub: 'A backup drive fixes that. The Backup page shows how to set one up.',
					action: openBackup,
				};
			case 'never':
				return {
					kind: 'backup-never',
					sentence: 'Your server hasn’t finished a backup yet.',
					sub: 'It backs up every night at 04:00 to each of your drives that is plugged in.',
					action: openBackup,
				};
			case 'stale':
				return {
					kind: 'backup-stale',
					sentence: `Your last backup finished ${backupAgo(backup.age_seconds)}.`,
					sub: 'Plug in a backup drive and the next nightly run brings it up to date.',
					action: openBackup,
				};
		}
	}

	if (appletsNeedingYou && appletsNeedingYou.length > 0) {
		const one = appletsNeedingYou.length === 1;
		return {
			kind: 'applets',
			sentence: one ? `${appletsNeedingYou[0]} needs you.` : 'Some of what your server runs needs you.',
			sub: one
				? 'Its last run didn’t finish, or a run is more than an hour late.'
				: 'Their last runs didn’t finish, or a run is more than an hour late.',
			action: { label: 'Open Jobs', route: '/virtues/jobs' },
		};
	}

	return { kind: 'well', sentence: 'Your server is running well.', sub: wellSub(disk, backup) };
}

/** The facts behind "running well", only the ones the page could read. */
function wellSub(disk: Disk | null, backup: BackupStatus | null): string {
	const diskPart = !disk
		? null
		: diskUsedPct(disk) < DISK_ROOMY_PCT
			? 'Its disk has plenty of room'
			: `Its disk has ${formatBytes(disk.available)} left`;
	const backupPart = backup?.state === 'ok' ? `last backup finished ${backupAgo(backup.age_seconds)}` : null;
	if (diskPart && backupPart) return `${diskPart}, and its ${backupPart}.`;
	if (diskPart) return `${diskPart}.`;
	if (backupPart) return `Its ${backupPart}.`;
	return 'Everything it checks is answering.';
}

/** The Updates row's value. Null when the status didn't load. */
export function updatesValue(status: UpdateStatus | null): string | undefined {
	if (!status) return undefined;
	if (status.check_error) return 'Couldn’t check';
	if (status.running_ahead) return 'Early build';
	if (status.update_available) {
		const name = status.latest ?? 'An update';
		return `${name} is ${status.staged ? 'ready' : 'available'}`;
	}
	return 'Up to date';
}

/** The Search row's value. `busy` means no answer within the probe's limit, often while indexing. */
export function searchValue(status: SearchStatus | null): string | undefined {
	if (!status) return undefined;
	if (status.busy) return 'Busy';
	return status.reachable ? 'Answering' : 'Not answering';
}

/** The Applets row's value, from the names of the applets that need you. */
export function appletsValue(needingYou: string[] | null): string | undefined {
	if (!needingYou) return undefined;
	if (needingYou.length === 0) return 'Running on schedule';
	return needingYou.length === 1 ? `${needingYou[0]} needs you` : 'Some need you';
}

/** "24 days", "3 hours", "12 minutes". */
export function upFor(secs: number): string {
	const days = Math.floor(secs / 86_400);
	if (days >= 1) return `${days} day${days === 1 ? '' : 's'}`;
	const hours = Math.floor(secs / 3600);
	if (hours >= 1) return `${hours} hour${hours === 1 ? '' : 's'}`;
	const minutes = Math.max(1, Math.floor(secs / 60));
	return `${minutes} minute${minutes === 1 ? '' : 's'}`;
}
