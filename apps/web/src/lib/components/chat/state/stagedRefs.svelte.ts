/**
 * stagedRefs — highlight-to-reference.
 *
 * Track D: select text in a message → comment bar → stage a reference chip
 * above the composer that scopes the next message. Empty note = quote; typed
 * note = quote + comment. Ephemeral. The in-text mark uses the app's own
 * --color-highlight token (one warm marker, not a per-ref rainbow) —
 * references are distinguished by being listed, not colored.
 *
 * Owns: the pending selection, the committed refs, and the painting of both.
 * The window mouseup listener stays on the view (it is `<svelte:window>`); this
 * module is what it calls.
 */

export type StagedRef = {
	id: string;
	messageId: string;
	text: string;
	range: Range;
};

export type SelectionDraft = {
	text: string;
	messageId: string;
	rect: { top: number; left: number; bottom: number; width: number };
	range: Range;
};

export class StagedRefsController {
	staged = $state<StagedRef[]>([]);
	draft = $state<SelectionDraft | null>(null);

	/** A mouseup anywhere: either it ends a selection inside a message (stage a
	 *  draft) or it is a plain click (dismiss the popover). */
	handleWindowMouseup(e: MouseEvent) {
		const sel = window.getSelection();
		const text = sel && !sel.isCollapsed ? sel.toString().trim() : "";
		if (text && sel && sel.rangeCount > 0) {
			const range = sel.getRangeAt(0);
			const node = range.commonAncestorContainer;
			const el = (node.nodeType === 1 ? node : node.parentElement) as HTMLElement | null;
			const wrapper = el?.closest(".message-wrapper") as HTMLElement | null;
			// Only chat messages; ignore selections inside the popover itself.
			if (!wrapper || el?.closest(".vref-bar")) return;
			const rect = range.getBoundingClientRect();
			this.draft = {
				text,
				messageId: wrapper.getAttribute("data-message-id") || "",
				rect: { top: rect.top, left: rect.left, bottom: rect.bottom, width: rect.width },
				range: range.cloneRange(),
			};
			return;
		}
		// Collapsed selection = a click → dismiss the popover if clicking outside it.
		if (this.draft && !(e.target as HTMLElement)?.closest(".vref-bar")) {
			this.draft = null;
		}
	}

	add() {
		if (!this.draft) return;
		const d = this.draft;
		this.staged = [
			...this.staged,
			{
				id: crypto?.randomUUID?.() ?? `ref-${this.staged.length}-${d.text.length}`,
				messageId: d.messageId,
				text: d.text,
				range: d.range,
			},
		];
		this.draft = null;
		window.getSelection()?.removeAllRanges();
		this.repaint();
	}

	remove(id: string) {
		this.staged = this.staged.filter((r) => r.id !== id);
		this.repaint();
	}

	clear() {
		this.staged = [];
		this.repaint();
	}

	// Paint staged refs with the CSS Custom Highlight API under one name — no DOM
	// mutation, no reflow, themed via --color-highlight. Only committed refs are
	// painted; the pending selection keeps the browser's own native highlight so
	// it never double-marks (and Cmd+C keeps copying it).
	repaint() {
		const cssAny = CSS as any;
		if (
			typeof CSS === "undefined" ||
			!cssAny.highlights ||
			typeof (window as any).Highlight === "undefined"
		)
			return;
		const ranges = this.staged.map((r) => r.range);
		if (ranges.length === 0) {
			cssAny.highlights.delete("vref");
			return;
		}
		try {
			cssAny.highlights.set("vref", new (window as any).Highlight(...ranges));
		} catch {
			/* range invalidated by a re-render — drop silently */
		}
	}

	/** The block prepended to the next message: each ref as a blockquote. */
	serialize(): string {
		return this.staged.map((r) => `> ${r.text.replace(/\s*\n\s*/g, " ")}`).join("\n\n");
	}
}
