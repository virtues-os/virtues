/**
 * Captures from the phone's drawer: a note, a photo or a file put into the
 * Inbox to be dealt with later.
 *
 * A capture is a task of the owner's, in no thread, due today, titled
 * "Inboxed: …", so it lands at the top of the Inbox's Today. A photo or a
 * file is stored first, in Storage's Inbox folder, and the task's notes link
 * to it. Filing it later is moving the task into a thread, or closing it.
 */
import { createTask, uploadDriveFile } from '$lib/api/client';

const FOLDER = 'Inbox';
const TITLE_CHARS = 60;

function today(): string {
	const d = new Date();
	const pad = (n: number) => String(n).padStart(2, '0');
	return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** A note typed (or dictated with the keyboard's mic). */
export async function captureNote(text: string): Promise<void> {
	const body = text.trim();
	if (!body) return;
	const firstLine = body.split('\n')[0];
	const title = firstLine.length > TITLE_CHARS ? `${firstLine.slice(0, TITLE_CHARS - 1).trimEnd()}…` : firstLine;
	await createTask({
		title: `Inboxed: ${title}`,
		notes: body === title ? null : body,
		due_on: today(),
	});
}

/** A photo or a file: stored in Storage, then a task that links to it. */
export async function captureFile(file: File): Promise<void> {
	const stored = await uploadDriveFile(FOLDER, file);
	await createTask({
		title: `Inboxed: ${stored.filename}`,
		notes: `[${stored.filename}](/drive/${stored.id})`,
		due_on: today(),
	});
}
