/**
 * What a `show` call returns, and the arithmetic its components share.
 *
 * The box shapes every result (virtues-core `tools/show.rs`): it ran the
 * query, refused anything it cannot draw, and turned numeric strings into
 * numbers. So nothing here validates; it only lays out.
 */

export type ShowKind = "chart" | "numbers" | "table" | "map" | "timeline" | "choices";

export type Row = Record<string, unknown>;

export interface ShowOutput {
	kind: ShowKind;
	title?: string | null;
	/** chart only */
	mark?: "bar" | "line";
	columns?: string[];
	rows?: Row[];
	row_count?: number;
	/** table only: the query had more rows than the table shows. */
	more?: boolean;
	/** choices only */
	options?: string[];
	/** The query's own ref, `/chat/{chat}/tool/{call}`, in a saved chat. */
	ref?: string;
}

/** Whether a tool output is something `Show` can draw. */
export function isShowOutput(o: unknown): o is ShowOutput {
	const kind = (o as ShowOutput | null)?.kind;
	return (
		typeof kind === "string" &&
		["chart", "numbers", "table", "map", "timeline", "choices"].includes(kind)
	);
}

/** Up to `count` round tick values covering [min, max]. */
export function niceTicks(min: number, max: number, count = 4): number[] {
	if (!Number.isFinite(min) || !Number.isFinite(max)) return [];
	if (min === max) {
		if (min === 0) return [0, 1];
		[min, max] = min > 0 ? [0, min] : [min, 0];
	}
	const raw = (max - min) / count;
	const mag = 10 ** Math.floor(Math.log10(raw));
	const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= raw) ?? 10 * mag;
	const start = Math.floor(min / step) * step;
	const ticks: number[] = [];
	for (let v = start; v <= max + step * 1e-9; v += step) ticks.push(round(v, step));
	if (ticks[ticks.length - 1] < max) ticks.push(round(start + ticks.length * step, step));
	return ticks;
}

function round(v: number, step: number): number {
	const digits = Math.max(0, -Math.floor(Math.log10(step)) + 1);
	return Number(v.toFixed(digits));
}

// Grouping from five digits, so a year or a count like 2026 reads 2026, not
// 2,026, and 12,400 still groups.
const plain = new Intl.NumberFormat(undefined, { maximumFractionDigits: 2, useGrouping: "min2" });
const whole = new Intl.NumberFormat(undefined, { maximumFractionDigits: 0, useGrouping: "min2" });
const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });

/** A number as a person reads it: 7.25, 1,204, 3.4M. */
export function formatNumber(n: number, { compactAbove = 1e6 } = {}): string {
	if (Math.abs(n) >= compactAbove) return compact.format(n);
	if (Math.abs(n) >= 100) return whole.format(n);
	return plain.format(n);
}

/** A cell as text: numbers formatted, times in the reader's own clock. */
export function formatCell(v: unknown): string {
	if (v === null || v === undefined) return "";
	if (typeof v === "number") return formatNumber(v, { compactAbove: Infinity });
	if (typeof v === "boolean") return v ? "Yes" : "No";
	if (typeof v === "string") {
		if (DATE.test(v)) return formatTime(parseTime(v)!, { withDate: true, withTime: false });
		const t = TIMESTAMP.test(v) ? parseTime(v) : null;
		if (t) return formatTime(t, { withDate: true });
		return v;
	}
	return JSON.stringify(v);
}

/** RFC 3339 (`timestamptz`) or the box's naive `YYYY-MM-DD HH:MM:SS`. */
const TIMESTAMP = /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/;
const DATE = /^\d{4}-\d{2}-\d{2}$/;

/**
 * A time the box wrote, or null. A bare date is midnight in the reader's
 * zone, not UTC: `new Date("2026-09-08")` is UTC and lands on the 7th
 * anywhere west of Greenwich. A naive timestamp is read the same way.
 */
export function parseTime(v: unknown): Date | null {
	if (typeof v !== "string") return null;
	if (DATE.test(v)) {
		const [y, m, d] = v.split("-").map(Number);
		return new Date(y, m - 1, d);
	}
	if (!TIMESTAMP.test(v)) return null;
	const hasZone = /(Z|[+-]\d{2}:?\d{2})$/.test(v);
	const t = new Date(hasZone ? v : v.replace(" ", "T"));
	return Number.isNaN(t.getTime()) ? null : t;
}

/** Whether every non-null value is a time, so the axis is a time axis. */
export function isTimeColumn(values: unknown[]): boolean {
	const present = values.filter((v) => v !== null && v !== undefined);
	return present.length > 0 && present.every((v) => parseTime(v) !== null);
}

/** Whether every value is a date with no time of day. */
export function isDateColumn(values: unknown[]): boolean {
	const present = values.filter((v) => v !== null && v !== undefined);
	return present.length > 0 && present.every((v) => typeof v === "string" && DATE.test(v));
}

export function formatTime(t: Date, { withDate = false, withTime = true } = {}): string {
	const date = t.toLocaleDateString(undefined, { month: "short", day: "numeric" });
	const midnight = t.getHours() === 0 && t.getMinutes() === 0;
	const time = t.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
	if (!withTime || (withDate && midnight)) return date;
	return withDate ? `${date}, ${time}` : time;
}

/** An x value as an axis label. */
export function formatX(v: unknown, dates: boolean): string {
	if (dates) {
		const t = parseTime(v);
		if (t) return formatTime(t, { withDate: true, withTime: false });
	}
	return formatCell(v);
}

/**
 * Indices of at most `max` labels spread evenly over `n` points, always
 * including the first and last, so labels never collide.
 */
export function labelIndices(n: number, max: number): number[] {
	if (n <= max) return Array.from({ length: n }, (_, i) => i);
	const step = (n - 1) / (max - 1);
	return Array.from({ length: max }, (_, i) => Math.round(i * step));
}

/**
 * Lanes for spans so none overlaps another in its lane: each takes the first
 * lane free at its start. `gap` is in the same units as the spans.
 */
export function assignLanes(spans: { start: number; end: number }[], gap = 0): number[] {
	const laneEnds: number[] = [];
	const order = spans.map((s, i) => i).sort((a, b) => spans[a].start - spans[b].start);
	const lanes = new Array<number>(spans.length);
	for (const i of order) {
		let lane = laneEnds.findIndex((end) => end + gap <= spans[i].start);
		if (lane === -1) lane = laneEnds.push(spans[i].end) - 1;
		else laneEnds[lane] = spans[i].end;
		lanes[i] = lane;
	}
	return lanes;
}

/** Whether a column's values are numbers, so it aligns right in a table. */
export function isNumericColumn(rows: Row[], column: string): boolean {
	const present = rows.map((r) => r[column]).filter((v) => v !== null && v !== undefined);
	return present.length > 0 && present.every((v) => typeof v === "number");
}

/**
 * A column as a person reads it. An alias the model wrote ("Hours asleep")
 * stands as written; a bare schema name (`calendar_name`) is the database
 * talking, so it becomes "Calendar name".
 */
export function columnLabel(column: string): string {
	if (!/^[a-z0-9_]+$/.test(column)) return column;
	const words = column.replace(/_+/g, " ").trim();
	return words.charAt(0).toUpperCase() + words.slice(1);
}

/** Whether a column has any value at all; an empty one is left out of a table. */
export function hasValues(rows: Row[], column: string): boolean {
	return rows.some((r) => r[column] !== null && r[column] !== undefined && r[column] !== "");
}

/** Whether every value in a column is a time, so its cells do not wrap. */
export function isTimeValues(rows: Row[], column: string): boolean {
	return isTimeColumn(rows.map((r) => r[column]));
}

/** A row's own record ref, set by the box when the query read one table. */
export function rowRef(row: Row): string | null {
	const ref = row.ref;
	return typeof ref === "string" && ref.startsWith("/") ? ref : null;
}
