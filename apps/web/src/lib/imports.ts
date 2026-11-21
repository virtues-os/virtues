/**
 * One-time imports: a file the owner downloads from a service and brings in.
 * Past chats (a Claude or ChatGPT export), Spotify listening, Amazon orders,
 * Discord messages and a Slack workspace export.
 *
 * The upload returns as soon as the import has started; the import runs as
 * that source's applet. `importFile` follows the run until it ends and
 * resolves with its one-line summary ("Brought in your ChatGPT chats from
 * March 2023 to November 2026."), or rejects with the applet's own reason.
 * Setup's chats step and the import card in Sources both use it.
 */
import { listRuns, uploadChatImport, uploadImport, type ImportResponse } from '$lib/api/client';

const POLL_MS = 1500;
/** Past the applets' own limits, the run has ended one way or another; stop
 *  asking. */
const FOLLOW_MS = 21 * 60_000;
/** Polls that may fail in a row before the server counts as unreachable. */
const MAX_MISSES = 10;

/** What the import card says for one source. */
export interface ImportSource {
	/** The window's title. */
	title: string;
	/** How to get the file from the service, as sentences. */
	how: string;
	/** The file picker's label. */
	fileLabel: string;
	/** `accept` for the file input. */
	accept: string;
	/** The button while the server reads the file. */
	reading: string;
	/** The line when the run ends without a summary of its own. */
	done: string;
	/** The source page's status line and the sentence under it. */
	line: string;
	why: string;
	/** Label of an email field the import needs, when it needs one, and the
	 *  sentence when it is left empty. */
	emailLabel?: string;
	emailMissing?: string;
	appletId: string;
	upload: (
		file: File,
		onProgress: (percent: number) => void,
		fields: Record<string, string>
	) => Promise<ImportResponse>;
}

export const IMPORT_SOURCES: Record<string, ImportSource> = {
	chat_import: {
		title: 'Bring in past chats',
		how: 'In ChatGPT, open Settings, then Data controls, then Export data. In Claude, open Settings, then Privacy, then Export data. Each one emails you a zip. Upload it here as it is.',
		fileLabel: 'Export file',
		accept: '.zip,application/zip,.json,application/json',
		reading: 'Reading your chats…',
		done: 'Your chats are in.',
		line: 'Bring in your past chats.',
		why: 'Export them from Claude or ChatGPT, then import the file here. A newer export adds only what is new.',
		appletId: 'applet_chat_import',
		upload: (file, onProgress) => uploadChatImport(file, onProgress)
	},
	spotify: {
		title: 'Bring in your Spotify listening',
		how: "On Spotify's website, open your account's privacy settings and request your extended streaming history. Spotify emails you a zip, which can take up to 30 days. Upload it here as it is.",
		fileLabel: 'Spotify download',
		accept: '.zip,application/zip,.json,application/json',
		reading: 'Reading your listening…',
		done: 'Your listening is in.',
		line: 'Bring in your Spotify listening.',
		why: 'Request your data from Spotify, then import the zip here. A newer download adds only what is new.',
		appletId: 'applet_spotify_import',
		upload: (file, onProgress) => uploadImport('spotify', file, onProgress)
	},
	amazon: {
		title: 'Bring in your Amazon orders',
		how: 'On Amazon, open Request your data from your account and ask for your orders. Amazon emails you a link to a zip, which can take a few days. Upload it here as it is.',
		fileLabel: 'Amazon download',
		accept: '.zip,application/zip,.csv,text/csv',
		reading: 'Reading your orders…',
		done: 'Your orders are in.',
		line: 'Bring in your Amazon orders.',
		why: 'Request your data from Amazon, then import the zip here. A newer download adds only what is new.',
		appletId: 'applet_amazon_orders_import',
		upload: (file, onProgress) => uploadImport('amazon', file, onProgress)
	},
	discord: {
		title: 'Bring in your Discord messages',
		how: "In Discord, open User Settings, then Data & Privacy, and request your data. Discord emails you a link to a zip with the messages you sent. Upload it here as it is.",
		fileLabel: 'Discord download',
		accept: '.zip,application/zip',
		reading: 'Reading your messages…',
		done: 'Your messages are in.',
		line: 'Bring in your Discord messages.',
		why: 'Request your data from Discord, then import the zip here. It has only the messages you sent. A newer download adds only what is new.',
		appletId: 'applet_discord_import',
		upload: (file, onProgress) => uploadImport('discord', file, onProgress)
	},
	slack: {
		title: 'Bring in a Slack workspace',
		how: "A workspace admin makes the export: in Slack, open the workspace's settings, then Import and export data, then Export. Slack emails a link to a zip. Upload it here as it is.",
		fileLabel: 'Slack export',
		accept: '.zip,application/zip',
		reading: 'Reading your messages…',
		done: 'Your messages are in.',
		line: 'Bring in a Slack workspace.',
		why: "Ask the workspace's admin for an export, then import the zip here. Only conversations you are in are kept. A newer export adds only what is new.",
		emailLabel: 'Your email in Slack',
		emailMissing: 'Enter the email you use in Slack, then import.',
		appletId: 'applet_slack_import',
		upload: (file, onProgress, fields) => uploadImport('slack', file, onProgress, fields)
	}
};

/** Whether a catalog source is brought in with a file rather than connected. */
export function isImportSource(id: string): boolean {
	return id in IMPORT_SOURCES;
}

/** Where an import is: the upload, with how much of it has gone, then the
 *  server reading the file. */
export type ImportPhase = { step: 'uploading'; percent: number } | { step: 'reading' };

/** The applet's own sentence from a failed run's stderr: anyhow prints it
 *  after "Error: ", with the causes below it. */
function reasonOf(raw: string | null | undefined): string {
	const line = raw?.split('\n').find((l) => l.startsWith('Error: '));
	return line ? line.slice('Error: '.length) : 'The import stopped before it finished. Try the file again.';
}

function wait(ms: number, signal?: AbortSignal): Promise<void> {
	return new Promise((resolve, reject) => {
		const t = setTimeout(resolve, ms);
		signal?.addEventListener(
			'abort',
			() => {
				clearTimeout(t);
				reject(new DOMException('Stopped following the import', 'AbortError'));
			},
			{ once: true }
		);
	});
}

/** Upload `file` for `sourceId` and follow its import to the end. An abort
 *  stops the following, never the upload or the import: both carry on, so
 *  leaving the page does not lose a file that was halfway up. */
export async function importFile(
	sourceId: string,
	file: File,
	opts: { onPhase?: (phase: ImportPhase) => void; signal?: AbortSignal; fields?: Record<string, string> } = {}
): Promise<string> {
	const source = IMPORT_SOURCES[sourceId];
	if (!source) throw new Error(`There is no import for ${sourceId}.`);
	opts.onPhase?.({ step: 'uploading', percent: 0 });
	const started = await source.upload(file, (percent) => opts.onPhase?.({ step: 'uploading', percent }), opts.fields ?? {});
	if (started.status === 'success') return started.summary ?? source.done;
	// The server runs one import of each kind at a time and turns the next away.
	if (started.status === 'skipped') {
		throw new Error('Another file is still being read. Add this one when it finishes.');
	}
	if (started.status !== 'running' || !started.run_id) throw new Error(reasonOf(started.error));

	opts.onPhase?.({ step: 'reading' });
	const until = Date.now() + FOLLOW_MS;
	let misses = 0;
	while (Date.now() < until) {
		await wait(POLL_MS, opts.signal);
		let runs;
		try {
			runs = await listRuns({ applet_id: source.appletId, limit: 10 });
			misses = 0;
		} catch {
			// A dropped poll is not a failed import; ask again, a few times.
			if (++misses >= MAX_MISSES) break;
			continue;
		}
		const run = runs.find((r) => r.id === started.run_id);
		if (!run || run.status === 'running') continue;
		if (run.status === 'success') return run.result_summary?.split('\n')[0] ?? source.done;
		throw new Error(reasonOf(run.error));
	}
	throw new Error('Lost track of the import. It may still finish; come back in a few minutes to see.');
}

export function isAbort(e: unknown): boolean {
	return e instanceof DOMException && e.name === 'AbortError';
}
