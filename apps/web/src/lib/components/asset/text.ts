/**
 * Text-asset helpers for AssetView panes: capped streaming fetch (the drive
 * download route has no Range support yet, so we read the stream and cancel
 * at the cap) and a small RFC-4180 delimited-text parser for CsvPane.
 */

export interface CappedText {
	text: string;
	truncated: boolean;
}

/** Fetch a URL as UTF-8 text, reading at most `capBytes`. Asks the server for
 * a byte range first (206 = exactly the cap, cheap); the cancel-at-cap stream
 * path below remains as the fallback for full 200 responses. */
export async function fetchTextCapped(url: string, capBytes: number): Promise<CappedText> {
	const res = await fetch(url, { headers: { Range: `bytes=0-${capBytes - 1}` } });
	// An empty file makes any range unsatisfiable — that's just empty text.
	if (res.status === 416) return { text: "", truncated: false };
	if (!res.ok) throw new Error(`Failed to load file (${res.status})`);
	if (res.status === 206) {
		// Content-Range: bytes 0-N/total — truncated iff the object outruns the cap.
		const total = Number(res.headers.get("content-range")?.split("/")[1] ?? NaN);
		const text = await res.text();
		return { text, truncated: Number.isFinite(total) && total > capBytes };
	}
	const reader = res.body?.getReader();
	if (!reader) {
		const text = await res.text();
		return { text: text.slice(0, capBytes), truncated: text.length > capBytes };
	}
	const chunks: Uint8Array[] = [];
	let received = 0;
	let truncated = false;
	for (;;) {
		const { done, value } = await reader.read();
		if (done) break;
		chunks.push(value);
		received += value.byteLength;
		if (received >= capBytes) {
			truncated = true;
			await reader.cancel();
			break;
		}
	}
	let buf = new Uint8Array(received);
	let offset = 0;
	for (const c of chunks) {
		buf.set(c, offset);
		offset += c.byteLength;
	}
	if (truncated) buf = buf.slice(0, capBytes);
	const text = new TextDecoder('utf-8', { fatal: false }).decode(buf);
	return { text, truncated };
}

/** Pick the likeliest delimiter for a file from its first line. */
export function sniffDelimiter(text: string, filename: string): string {
	if (/\.tsv$/i.test(filename)) return '\t';
	const firstLine = text.slice(0, text.indexOf('\n') === -1 ? text.length : text.indexOf('\n'));
	let best = ',';
	let bestCount = -1;
	for (const d of [',', '\t', ';']) {
		const count = firstLine.split(d).length - 1;
		if (count > bestCount) {
			best = d;
			bestCount = count;
		}
	}
	return best;
}

/**
 * RFC-4180 parse: quoted fields, doubled-quote escapes, newlines inside
 * quotes, CRLF line endings. Stops after `maxRows` rows.
 */
export function parseDelimited(text: string, delimiter: string, maxRows: number): string[][] {
	const rows: string[][] = [];
	let row: string[] = [];
	let field = '';
	let inQuotes = false;
	for (let i = 0; i < text.length; i++) {
		const ch = text[i];
		if (inQuotes) {
			if (ch === '"') {
				if (text[i + 1] === '"') {
					field += '"';
					i++;
				} else {
					inQuotes = false;
				}
			} else {
				field += ch;
			}
		} else if (ch === '"') {
			inQuotes = true;
		} else if (ch === delimiter) {
			row.push(field);
			field = '';
		} else if (ch === '\n' || ch === '\r') {
			if (ch === '\r' && text[i + 1] === '\n') i++;
			row.push(field);
			field = '';
			rows.push(row);
			row = [];
			if (rows.length >= maxRows) return rows;
		} else {
			field += ch;
		}
	}
	if (field !== '' || row.length > 0) {
		row.push(field);
		rows.push(row);
	}
	return rows;
}
