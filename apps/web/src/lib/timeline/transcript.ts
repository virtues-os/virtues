/**
 * transcript.ts - a conversation's words as the inspector reads them
 * (dayback/src/main.js:1549-1555, 1616-1626): speaker turns, never
 * identities, and each line at an estimated moment, since the recorder keeps
 * no per-line times.
 */

export interface Line {
	/** "A", "B", ... for a diarised speaker; a name passes through; "" untagged. */
	spk: string;
	txt: string;
}

/** A window's utterances: one per line when speaker-tagged; a single untagged
 *  blob is read one sentence at a time. */
export function convLines(text: string | null): string[] {
	const raw = (text ?? "").split("\n").filter((l) => l.trim());
	if (raw.length !== 1) return raw;
	return (raw[0].match(/[^.!?]+[.!?]+["'”]?|[^.!?]+$/g) ?? [raw[0]]).map((x) => x.trim()).filter(Boolean);
}

/** "[Speaker 1]: hello" reads as speaker "A" saying "hello". */
export function parseLine(l: string): Line {
	const m = l.match(/^\[([^\]]+)\]:\s*(.*)$/);
	let spk = m ? m[1] : "";
	const n = spk.match(/^Speaker\s+(\d+)$/i);
	if (n) spk = String.fromCharCode(64 + Number(n[1]));
	return { spk, txt: m ? m[2] : l };
}

/** A row's transcript: every window's lines in order. Each window shifts its
 *  letters by two (A/B, then C/D, ...): the recorder recounts its speakers
 *  per window, so a new window never claims to be the same person across the
 *  seam. */
export function rowLines(windows: { text: string | null }[]): Line[] {
	const out: Line[] = [];
	windows.forEach((w, wi) => {
		for (const l of convLines(w.text)) {
			const q = parseLine(l);
			if (/^[A-Z]$/.test(q.spk)) q.spk = String.fromCharCode(Math.min(90, q.spk.charCodeAt(0) + wi * 2));
			out.push(q);
		}
	});
	return out;
}

/** One of four speaker tints: a letter by its place, a name by its hash. */
export function spkIdx(s: string): number {
	if (/^[A-Z]$/.test(s)) return (s.charCodeAt(0) - 65) % 4;
	let h = 0;
	for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
	return Math.abs(h) % 4;
}

/** Line `i` of `n` at its estimated moment: evenly spread over [s, e). */
export function lineTime(s: number, e: number, i: number, n: number): number {
	return Math.round(s + ((i + 0.5) / n) * ((e - s) || 1));
}

/** The line being spoken at `t`: the one whose estimated moment is nearest. */
export function lineAt(s: number, e: number, n: number, t: number): number {
	let best = 0;
	let bd = Infinity;
	for (let i = 0; i < n; i++) {
		const d = Math.abs(lineTime(s, e, i, n) - t);
		if (d < bd) {
			bd = d;
			best = i;
		}
	}
	return best;
}
