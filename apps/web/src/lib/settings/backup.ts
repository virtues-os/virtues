/**
 * Backup's words, shared by the Backup page, the Backup row on Your data and
 * the Backup row on Status, so the three say the same thing about the same
 * state (`api/backup_status.rs`: none, never, ok, stale, failing).
 */
import type { BackupStatus } from '$lib/api/client';

/** "under an hour ago", "4 hours ago", "9 days ago". */
export function backupAgo(secs: number | null): string {
	if (secs === null) return 'never';
	const h = Math.floor(secs / 3600);
	if (h < 1) return 'under an hour ago';
	if (h < 48) return `${h} hour${h === 1 ? '' : 's'} ago`;
	return `${Math.floor(h / 24)} days ago`;
}

/** One short line for a row that opens the Backup page. */
export function backupSummary(status: BackupStatus): string {
	switch (status.state) {
		case 'none':
			return 'No backup drive yet';
		case 'never':
			return 'No backup yet';
		case 'failing':
			return 'Last backup failed';
		default:
			return `Last backup ${backupAgo(status.age_seconds)}`;
	}
}
