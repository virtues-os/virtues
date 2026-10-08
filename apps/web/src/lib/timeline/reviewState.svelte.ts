/**
 * reviewState.svelte.ts - "Help Virtues out", as state the day's list and its
 * Review share: the questions and the places named without asking, an answer
 * and its Undo, and pointing to Home or Work on the map. The questions
 * themselves are review.ts's; this holds what the person has done with them.
 */
import { fetchReview, sendAnswer, type Answer, type PlaceReview } from "./review";

/** How long an answer can be undone. */
const UNDO_MS = 8000;

/** What an answer said, for its Undo line. */
export function said(a: Answer): string {
	switch (a.kind) {
		case "name":
			return `Saved as ${a.name}`;
		case "passing":
			return "Saved as passing through";
		case "home":
		case "home_here":
			return "Saved as Home";
		case "work":
		case "work_here":
			return "Saved as where you work";
		case "not_home":
			return "Saved: not home";
		case "not_work":
			return "Saved: not where you work";
		case "not_this":
			return `Saved: not ${a.name}`;
		default:
			return "Saved";
	}
}

export class ReviewState {
	review = $state<PlaceReview | null>(null);
	failed = $state(false);
	saving = $state(false);
	/** The place whose answer the server refused. */
	saveFailed = $state<string | null>(null);
	/** The last answer, undoable for a few seconds: the place it was given
	 *  at, what it said, and the places it touched. */
	undo = $state<{ at: string; said: string; ids: string[] } | null>(null);
	/** "No" to Home or Work: the place said no to, and the role being
	 *  pointed to on the map. */
	pointing = $state<{ id: string; role: "home" | "work" } | null>(null);

	questions = $derived(this.review?.questions ?? []);
	confirmed = $derived(this.review?.confirmed ?? []);
	byPlace = $derived(new Map(this.questions.map((q) => [q.id, q])));
	homeSet = $derived(this.confirmed.some((c) => c.source === "home"));

	private undoTimer = 0;

	/** `onanswered`: an answer was saved or undone, so the day's titles may
	 *  have changed. */
	constructor(private onanswered: () => void) {}

	async load() {
		try {
			this.review = await fetchReview();
			this.failed = false;
		} catch (e) {
			console.warn("[Timeline] place review fetch failed", e);
			this.failed = true;
		}
	}

	async answer(id: string, a: Answer): Promise<boolean> {
		if (this.saving) return false;
		this.saving = true;
		this.saveFailed = null;
		try {
			const done = await sendAnswer(id, a);
			this.pointing = null;
			await this.load();
			clearTimeout(this.undoTimer);
			this.undo = a.kind === "reopen" ? null : { at: id, said: said(a), ids: [id, ...(done.place_id ? [done.place_id] : [])] };
			this.undoTimer = window.setTimeout(() => (this.undo = null), UNDO_MS);
			this.onanswered();
			return true;
		} catch (e) {
			console.warn("[Timeline] place answer failed", e);
			this.saveFailed = id;
			return false;
		} finally {
			this.saving = false;
		}
	}

	/** Every answer is about a place that was a question or named; forgetting
	 *  it puts each place it touched back where it was. */
	async undoLast() {
		const u = this.undo;
		if (!u) return;
		this.undo = null;
		clearTimeout(this.undoTimer);
		for (const id of [...u.ids].reverse()) await this.answer(id, { kind: "reopen" });
		this.undo = null;
	}

	startPointing(id: string, role: "home" | "work") {
		this.pointing = { id, role };
	}

	cancelPointing() {
		this.pointing = null;
	}

	/** Home or Work is at the spot tapped on the map. */
	savePointed(at: { lat: number; lon: number }) {
		const p = this.pointing;
		if (p) void this.answer(p.id, { kind: p.role === "home" ? "home_here" : "work_here", ...at });
	}

	/** Not this place, and not asked again unless another takes over. */
	skipPointing() {
		const p = this.pointing;
		if (p) void this.answer(p.id, { kind: p.role === "home" ? "not_home" : "not_work" });
	}
}
