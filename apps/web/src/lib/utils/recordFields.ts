/**
 * How a raw record row reads to a person: its title, its date, and the fields
 * worth showing. Shared by the record viewer (DataView) and the citation card,
 * so a record names itself the same way in both.
 */

import type { OntologyRecord } from '$lib/api/client';

/** Columns a reader never needs: search plumbing and opaque blobs. */
export const HIDDEN_FIELD = /embedding|vector|tsv|_hash$|bm25|search_/i;

/** Candidate keys for a record's human title, in priority order. */
export const TITLE_KEYS = [
	'title',
	'subject',
	'name',
	'merchant_name',
	'merchant',
	'summary',
	'headline',
	'label',
	'description',
];

export function recordTitle(row: Record<string, unknown>, fallback: string): string {
	for (const k of TITLE_KEYS) {
		const v = row[k];
		if (typeof v === 'string' && v.trim()) return v.trim();
	}
	return fallback;
}

export function humanizeKey(key: string): string {
	return key.replace(/_/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase());
}

export function looksLikeDate(s: string): boolean {
	return /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/.test(s);
}

export function formatRecordDate(v: string | number): string {
	const d = new Date(v);
	if (Number.isNaN(d.getTime())) return String(v);
	return d.toLocaleString(undefined, {
		weekday: 'short',
		year: 'numeric',
		month: 'short',
		day: 'numeric',
		hour: 'numeric',
		minute: '2-digit',
	});
}

export type RenderedValue = {
	kind: 'text' | 'date' | 'bool' | 'num' | 'json' | 'link';
	text: string;
};

export function renderValue(value: unknown): RenderedValue {
	if (typeof value === 'boolean') return { kind: 'bool', text: value ? 'Yes' : 'No' };
	if (typeof value === 'number') return { kind: 'num', text: value.toLocaleString() };
	if (typeof value === 'string') {
		if (looksLikeDate(value)) return { kind: 'date', text: formatRecordDate(value) };
		// A record whose whole point is an address — a bookmark, an asset —
		// is useless if the address is only selectable text. Linkifying is
		// generic on purpose: any ontology with a url field benefits.
		if (/^https?:\/\//i.test(value)) return { kind: 'link', text: value };
		return { kind: 'text', text: value };
	}
	if (value && typeof value === 'object') return { kind: 'json', text: JSON.stringify(value, null, 2) };
	return { kind: 'text', text: String(value) };
}

export type RecordField = { key: string; label: string; value: unknown };

/**
 * Everything meaningful in the row: the timestamp column first, minus the id,
 * the hidden plumbing, empty values and the value already used as the title.
 */
export function recordFields(record: OntologyRecord, title: string): RecordField[] {
	const row = record.row;
	const tsCol = record.timestamp_column;
	const usedTitleKey = TITLE_KEYS.find((k) => typeof row[k] === 'string' && (row[k] as string).trim() === title);
	const entries = Object.entries(row).filter(
		([k, v]) =>
			!HIDDEN_FIELD.test(k) &&
			k !== 'id' &&
			k !== usedTitleKey &&
			v !== null &&
			v !== '' &&
			!(Array.isArray(v) && v.length === 0)
	);
	entries.sort(([a], [b]) => {
		if (a === tsCol) return -1;
		if (b === tsCol) return 1;
		return 0;
	});
	return entries.map(([key, value]) => ({ key, label: humanizeKey(key), value }));
}
