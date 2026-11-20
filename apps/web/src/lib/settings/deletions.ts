/**
 * Deletions in words, for the chat's deletion card and Settings · Deleted
 * records. A deletion's receipt holds counts and dates, never the records
 * themselves (`virtues-core/src/api/deletions.rs`).
 *
 * Dates are written in US English, as `formatDate` writes them, so they read
 * the same as the sentences around them whatever the browser's language.
 */
import type { Deletion, DeletionReceipt } from '$lib/api/deletions';

/** "a, b and c". */
export function listed(parts: string[]): string {
	if (parts.length < 2) return parts.join('');
	return `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1]}`;
}

const sameYear = (a: Date, b: Date) => a.getFullYear() === b.getFullYear();

/** "November 12", or "November 12, 2025" when it isn't this year. */
export function dayInWords(at: string | null | undefined, now = Date.now()): string {
	if (!at) return '';
	const d = new Date(at);
	if (isNaN(d.getTime())) return '';
	return d.toLocaleDateString('en-US', sameYear(d, new Date(now)) ? { month: 'long', day: 'numeric' } : { month: 'long', day: 'numeric', year: 'numeric' });
}

const full = (d: Date) => d.toLocaleDateString('en-US', { month: 'long', day: 'numeric', year: 'numeric' });

/** "March 2 to March 30, 2025", "December 30, 2024 to January 2, 2025", or one day. */
export function span(earliest: string | null | undefined, latest: string | null | undefined): string {
	const from = earliest ? new Date(earliest) : null;
	const to = latest ? new Date(latest) : null;
	if (!from && !to) return '';
	if (!from || !to || full(from) === full(to)) return full((from ?? to) as Date);
	const start = sameYear(from, to) ? from.toLocaleDateString('en-US', { month: 'long', day: 'numeric' }) : full(from);
	return `${start} to ${full(to)}`;
}

export const recordsWord = (n: number) => `${n.toLocaleString()} ${n === 1 ? 'record' : 'records'}`;

/** "380 search entries, 12 links to people and places and 3 files". */
export function alsoRemoved(receipt: DeletionReceipt): string {
	const parts: string[] = [];
	const add = (n: number | undefined, one: string, many: string) => {
		if (n) parts.push(`${n.toLocaleString()} ${n === 1 ? one : many}`);
	};
	add(receipt.search_entries, 'search entry', 'search entries');
	add(receipt.links, 'link to a person or place', 'links to people and places');
	add(receipt.files, 'file', 'files');
	add(receipt.raw_records, 'raw copy', 'raw copies');
	return listed(parts);
}

/** Where copies remain, from what the receipt says the deletion did not reach. */
export function stillKept(receipt: DeletionReceipt): string {
	const words: Record<string, string> = {
		backups: 'in backups made before this',
		pages_and_chats: 'in pages or chats that already quoted them',
	};
	return listed((receipt.kept ?? []).map((k) => words[k]).filter(Boolean));
}

/** The summary as a title: the assistant writes it lowercase, for "Delete …?". */
export function deletionTitle(d: Pick<Deletion, 'summary'>): string {
	const s = d.summary.trim();
	return s ? s.charAt(0).toUpperCase() + s.slice(1) : 'A deletion';
}

/** When it ran: the day you allowed it, which the list is sorted by and which never moves. */
const ranAt = (d: Deletion) => d.approved_at ?? d.created_at;

export function deletionDay(d: Deletion, now = Date.now()): string {
	return dayInWords(ranAt(d), now);
}

/** "Deleted October 3, 2026, after you asked in a chat.", or "Started" for one that hasn't finished. */
export function deletionLede(d: Deletion): string {
	const verb = d.status === 'done' ? 'Deleted' : 'Started';
	const day = new Date(ranAt(d));
	const when = isNaN(day.getTime()) ? '' : ` ${full(day)}`;
	return `${verb}${when}${d.chat_id ? ', after you asked in a chat' : ''}.`;
}

/** A deletion that stopped before it finished, whether or not you then set it aside. */
export const stopped = (d: Deletion) => d.status === 'failed' || d.status === 'cancelled';

/**
 * One line under a deletion in the list: "October 3, from a chat. 214
 * records.", what stopped it, or that you let its records back. A deletion
 * made in Query says so in its summary; one whose chat was deleted since has
 * no chat to name.
 */
export function deletionLine(d: Deletion, now = Date.now()): string {
	const where = d.chat_id ? ', from a chat' : '';
	const day = `${deletionDay(d, now)}${where}.`;
	if (d.status === 'running') return `${day} Deleting now.`;
	if (d.status === 'failed') return `${day} Stopped: ${d.error ?? 'your server didn’t say why.'}`;
	if (d.status === 'cancelled') return `${day} Stopped, and you set it aside.`;
	const parts = [day, `${recordsWord(d.receipt.records ?? 0)}.`];
	if (d.receipt.tombstones_lifted !== undefined) {
		const when = dayInWords(d.receipt.lifted_at, now);
		parts.push(when ? `On ${when} you let them be collected again.` : 'You let them be collected again.');
	}
	return parts.join(' ');
}

/** The sentence a deletion's own page opens with. */
export function deletionLead(d: Deletion): string {
	if (d.status === 'running') return 'Your server is deleting these now.';
	if (d.status === 'failed') return 'This deletion stopped before it finished.';
	if (d.status === 'cancelled') return 'This deletion stopped, and you set it aside.';
	const n = d.receipt.records ?? 0;
	if (n === 0) return 'Nothing matched by the time it ran, so nothing was removed.';
	const when = span(d.receipt.earliest, d.receipt.latest);
	return `Your server deleted ${recordsWord(n)}${when ? `, from ${when}` : ''}.`;
}
