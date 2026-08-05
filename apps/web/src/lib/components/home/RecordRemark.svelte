<!--
	RecordRemark.svelte — the one thing the record has to say today.

	Everywhere else the box shows and the person reads. This is the box
	speaking: a single declarative line, counted rather than written, about
	something in the last two years you could not have noticed yourself.

	It is deterministic on purpose — no model is consulted and none could be,
	because the whole value of the sentence is that it is arithmetic. Two
	shapes are looked for, and only these two:

	  · a run of consecutive days above the two-year median for a measure
	  · yesterday standing higher than every day for some number of days back

	Zeros are days the collector was off, not days you took no steps, so they
	are excluded from the median and they break a run — which is correct
	either way, since neither claim survives a day with no evidence.

	When nothing clears the thresholds it renders nothing at all. A page that
	must have a remark every day ends up inventing them, and an invented
	remark costs more trust than a silent day costs interest.
-->
<script lang="ts">
	import { getLifeline } from "$lib/wiki/api";

	interface Props {
		/** Local midnight that starts today. Yesterday is the last full day. */
		dayStartMs: number;
	}
	let { dayStartMs }: Props = $props();

	/**
	 * Two years, one bucket a day, ending at this morning — so the last bucket
	 * is yesterday and every bucket in the series is a day that finished.
	 *
	 * Bucket edges are fixed 24h apart while local days are 23 or 25 around a
	 * DST change, so twice a year an hour lands in its neighbour. At the size
	 * of the claims below that is noise, and it is the only inaccuracy here.
	 */
	const WINDOW = 730;

	/** Enough history to have a median worth being above. */
	const MIN_HISTORY = 60;
	const MIN_ACTIVE = 45;
	/** A run shorter than this is a coincidence; a rank lower is a Tuesday. */
	const MIN_RUN = 3;
	const MIN_RANK = 30;

	let remark = $state<string | null>(null);

	const int = new Intl.NumberFormat();
	const ORD = [
		"",
		"First",
		"Second",
		"Third",
		"Fourth",
		"Fifth",
		"Sixth",
		"Seventh",
		"Eighth",
		"Ninth",
		"Tenth",
		"Eleventh",
		"Twelfth",
		"Thirteenth",
		"Fourteenth",
		"Fifteenth",
		"Sixteenth",
		"Seventeenth",
		"Eighteenth",
		"Nineteenth",
		"Twentieth",
	];
	function ordinal(n: number): string {
		if (ORD[n]) return ORD[n];
		// Past twenty the word is worse than the figure, but the figure still
		// needs the right tail: 21st, 22nd, 23rd, 24th — and 111th, 112th, 113th.
		const t = n % 100;
		const s = t >= 11 && t <= 13 ? "th" : ["th", "st", "nd", "rd"][n % 10] ?? "th";
		return `${n}${s}`;
	}

	/**
	 * A count keeps its precision at human scale and loses it at machine scale:
	 * 2.5 hours of screen time rounded to "3 h" is a different claim, and
	 * 12,412.4 steps is a false one.
	 */
	function num(v: number): string {
		return v >= 100 ? int.format(Math.round(v)) : String(Math.round(v * 10) / 10);
	}
	/** The registry's units are abbreviations; a sentence wants the word. */
	const UNITS: Record<string, string> = { h: "hours", hr: "hours", min: "minutes", m: "minutes" };

	/** A number with its unit, skipping the unit when the label already says it. */
	function amount(v: number, unit: string, label: string): string {
		const u = (unit ?? "").toLowerCase();
		if ((u === "min" || u === "m") && v >= 90) return `${num(v / 60)} hours`;
		if (!u || label.toLowerCase().includes(u)) return num(v);
		return `${num(v)} ${UNITS[u] ?? unit}`;
	}
	function cap(s: string): string {
		return s.charAt(0).toUpperCase() + s.slice(1);
	}

	type Cand = { text: string; score: number };

	function readLane(density: number[], label: string, unit: string): Cand[] {
		const out: Cand[] = [];
		const n = density.length;
		if (n < MIN_HISTORY) return out;

		const first = density.findIndex((v) => v > 0);
		if (first < 0 || n - first < MIN_HISTORY) return out;

		const active = density.slice(first).filter((v) => v > 0);
		if (active.length < MIN_ACTIVE) return out;

		const sorted = [...active].sort((a, b) => a - b);
		const med = sorted[Math.floor(sorted.length / 2)];
		if (!(med > 0)) return out;

		const lower = label.toLowerCase();

		// ── a run of days above the median ──
		let run = 0;
		for (let i = n - 1; i >= first && density[i] > med; i--) run++;
		if (run >= MIN_RUN) {
			// The longest run strictly before this one decides whether the
			// current one is worth calling notable.
			let prevMax = 0,
				cur = 0;
			for (let i = first; i < n - run; i++) {
				if (density[i] > med) cur++;
				else {
					prevMax = Math.max(prevMax, cur);
					cur = 0;
				}
			}
			prevMax = Math.max(prevMax, cur);
			const notable = run > prevMax ? " — the longest such run in that window" : "";
			if (run >= 4 || notable) {
				// Which median matters: a run above a two-year line is a smaller
				// claim than a run above this month's, and the sentence should
				// not let the reader assume the larger one.
				out.push({
					text: `${ordinal(run)} consecutive day above the two-year median for ${lower}${notable}.`,
					score: run * 25 + (notable ? 40 : 0),
				});
			}
		}

		// ── yesterday against the days behind it ──
		const v = density[n - 1];
		if (v > 0) {
			let rank = 0,
				capped = false;
			let i = n - 2;
			for (; i >= first; i--) {
				if (density[i] >= v) break;
				rank++;
			}
			if (i < first) capped = true;
			if (rank >= MIN_RANK) {
				const since = capped ? "two years" : `${rank} days`;
				out.push({
					text: `${cap(label)} yesterday: ${amount(v, unit, label)} — the most in ${since}.`,
					score: rank + (capped ? 200 : 0),
				});
			}
		}

		return out;
	}

	/**
	 * The measures worth a sentence. The lifeline answers with every lane it
	 * has, and a lane nobody chose a measure for falls back to counting rows —
	 * "Records yesterday: 412, the most in 74 days" is arithmetic about the
	 * collector, not about the day, so only what was actually asked for counts.
	 */
	const ASKED: Record<string, string> = {
		health: "steps",
		activity: "screen",
		communication: "sent",
	};

	$effect(() => {
		let dropped = false;
		const to = new Date(dayStartMs).toISOString();
		const from = new Date(dayStartMs - WINDOW * 86_400_000).toISOString();
		getLifeline(WINDOW, from, to, undefined, ASKED)
			.then((l) => {
				if (dropped || !l) return;
				const cands: Cand[] = [];
				for (const lane of l.lanes) {
					if (lane.measure !== ASKED[lane.id]) continue;
					// A rate is an average over its bucket; "the most in 74 days"
					// is a claim about a total and does not survive the change.
					if (lane.kind !== "total") continue;
					if (!lane.density || lane.density.length !== l.buckets) continue;
					cands.push(...readLane(lane.density, lane.measure_label || lane.id, lane.unit || ""));
				}
				cands.sort((a, b) => b.score - a.score);
				remark = cands[0]?.text ?? null;
			})
			.catch(() => {});
		return () => {
			dropped = true;
		};
	});
</script>

{#if remark}
	<!--
		No kicker. The typography says who is talking: this is the counted voice,
		tabular and flat, next to the serif line the record writes about itself.
	-->
	<p class="remark">{remark}</p>
{/if}

<style>
	.remark {
		margin: 0 0 26px;
		padding-top: 14px;
		border-top: 1px solid var(--color-border);
		max-width: 58ch;
		font-family: var(--font-sans);
		font-size: 15px;
		line-height: 1.5;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}
</style>
