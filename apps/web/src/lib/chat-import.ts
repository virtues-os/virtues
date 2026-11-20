/**
 * Bring a chat export into the server: the zip Claude or ChatGPT emails, or
 * the conversations.json inside it.
 *
 * The upload returns as soon as the import has started; the import itself
 * runs as the `chat_import` applet. This follows that run until it ends and
 * resolves with its one-line summary ("Brought in your ChatGPT chats from
 * March 2023 to November 2026."), or rejects with the applet's own reason.
 * Setup's chats step and the import card in Sources both use it.
 */
import { listRuns, uploadChatImport } from '$lib/api/client';

const APPLET_ID = 'applet_chat_import';
const POLL_MS = 1500;
/** Past the applet's own 10-minute limit, the run has ended one way or
 *  another; stop asking. */
const FOLLOW_MS = 11 * 60_000;
/** Polls that may fail in a row before the server counts as unreachable. */
const MAX_MISSES = 10;

/** Where an import is: the upload, with how much of it has gone, then the
 *  server reading the chats. */
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

/** Upload `file` and follow its import to the end. An abort stops the
 *  following, never the upload or the import: both carry on, so leaving the
 *  page does not lose an export that was halfway up. */
export async function importChatExport(
	file: File,
	opts: { onPhase?: (phase: ImportPhase) => void; signal?: AbortSignal } = {}
): Promise<string> {
	opts.onPhase?.({ step: 'uploading', percent: 0 });
	const started = await uploadChatImport(file, (percent) => opts.onPhase?.({ step: 'uploading', percent }));
	if (started.status === 'success') return started.summary ?? 'Your chats are in.';
	// The server runs one import at a time and turns the next away.
	if (started.status === 'skipped') {
		throw new Error('Another export is still being read. Add this one when it finishes.');
	}
	if (started.status !== 'running' || !started.run_id) throw new Error(reasonOf(started.error));

	opts.onPhase?.({ step: 'reading' });
	const until = Date.now() + FOLLOW_MS;
	let misses = 0;
	while (Date.now() < until) {
		await wait(POLL_MS, opts.signal);
		let runs;
		try {
			runs = await listRuns({ applet_id: APPLET_ID, limit: 10 });
			misses = 0;
		} catch {
			// A dropped poll is not a failed import; ask again, a few times.
			if (++misses >= MAX_MISSES) break;
			continue;
		}
		const run = runs.find((r) => r.id === started.run_id);
		if (!run || run.status === 'running') continue;
		if (run.status === 'success') return run.result_summary?.split('\n')[0] ?? 'Your chats are in.';
		throw new Error(reasonOf(run.error));
	}
	throw new Error('Lost track of the import. It may still finish; come back in a few minutes to see.');
}

export function isAbort(e: unknown): boolean {
	return e instanceof DOMException && e.name === 'AbortError';
}
