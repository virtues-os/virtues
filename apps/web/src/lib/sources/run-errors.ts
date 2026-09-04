/**
 * Turn a raw run error into the condition it names, when we can name one.
 *
 * A source run that fails on a macOS-protected path records the OS's sentence
 * verbatim — "I/O error: Permission denied (os error 13)" — and the run feed
 * used to print exactly that, ten times, to someone whose actual problem was a
 * checkbox in System Settings. The string itself often carries no path at all,
 * so the pattern alone cannot say WHICH permission; the collector's own health
 * report (health.json → `app_device.device_info.permissions`) is what turns
 * "some permission, somewhere" into "Full Disk Access, on that Mac".
 *
 * Two signals, either sufficient:
 *  - the error names a TCC-gated path (chat.db, Library/Safari, …), or
 *  - the error is a bare permission failure AND the source's collector has
 *    that permission on record as denied.
 * A bare permission failure with a healthy collector stays raw — it may be a
 * box-side fault (a root-owned lake dir has produced this exact string), and
 * naming the wrong remedy is worse than naming none.
 */
import { PERMISSION_COPY } from '$lib/devices/shared';

export interface RunErrorExplanation {
	/** Capability name as the collector reports it, e.g. `full_disk_access`. */
	permission: string;
	/** Human name, e.g. "Full Disk Access". */
	label: string;
	/** Headline for the run entry, e.g. "Full Disk Access needed". */
	title: string;
	/** The fix, in the owner's terms. */
	remedy: string;
	/** True when the collector's current health report confirms the denial. */
	confirmed: boolean;
	/** Deep link to the settings pane, meaningful only on the machine itself. */
	open?: () => Promise<boolean>;
}

/** The OS refusing access, in the shapes our Rust and sqlite errors take. */
const PERMISSION_FAILURE =
	/permission denied|os error 13|operation not permitted|os error 1\b|unable to open database file|not authorized/i;

/**
 * Paths macOS gates behind a TCC permission. When the error names one, the
 * classification stands on its own — no health report needed.
 */
const TCC_PATHS: { pattern: RegExp; permission: string }[] = [
	{ pattern: /chat\.db|Library\/Messages/i, permission: 'full_disk_access' },
	{ pattern: /Library\/Safari|History\.db/i, permission: 'full_disk_access' },
	{ pattern: /Library\/Mail/i, permission: 'full_disk_access' }
];

/** Permissions whose absence surfaces as a file-read failure. */
const FILE_READ_PERMISSIONS = ['full_disk_access'];

/**
 * Explain a run's error, or return null when the raw text is the best we have.
 *
 * `deniedNow` is the source's collector self-report — `Connection.denied` from
 * the sources store: the most recent report, possibly stale, and empty for
 * credential sources or collectors too old to report health at all.
 */
export function explainRunError(
	error: string | null,
	deniedNow: string[]
): RunErrorExplanation | null {
	if (!error || !PERMISSION_FAILURE.test(error)) return null;

	const byPath = TCC_PATHS.find((t) => t.pattern.test(error));
	const permission =
		byPath?.permission ?? FILE_READ_PERMISSIONS.find((p) => deniedNow.includes(p)) ?? null;
	if (!permission) return null;

	const copy = PERMISSION_COPY[permission];
	const label = copy?.label ?? permission;
	const confirmed = deniedNow.includes(permission);
	return {
		permission,
		label,
		title: `${label} needed`,
		remedy: confirmed
			? `The collector's last report says it doesn't have ${label}. Grant it in System Settings → Privacy & Security on the Mac that runs it.`
			: `macOS refused a protected file. Grant ${label} in System Settings → Privacy & Security on the Mac that runs the collector.`,
		confirmed,
		open: copy?.open
	};
}
