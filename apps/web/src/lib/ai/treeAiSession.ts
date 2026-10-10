/**
 * The inline writer on a block page: ⌘J, `/ai` and Ask AI in `DocumentEditor`
 * (`TreeAiDriver`). The CodeMirror editor's writer is `aiCursorSession.ts`.
 *
 * It streams a completion into the page's tree so it reads as writing:
 *
 *  - Where it writes is a Yjs relative position, so edits other devices make
 *    meanwhile move it, and a paragraph added above does not shift the text
 *    into the wrong place.
 *  - Chunks land every 30 ms as ordinary local transactions: they sync to
 *    every device, and the whole session is one undo step.
 *  - A line break in the output starts a new block. On a rewrite, the block
 *    the rewrite began in is never split, so rejecting it leaves that block
 *    whole: what follows the break there is proposed out of it and proposed
 *    again at the end of a new block, and later breaks split only blocks the
 *    rewrite made. A tail holding anything but text (a mention, a line
 *    break) cannot carry the marks, and the break becomes a space.
 *  - A CONTINUATION inserts plainly: there is nothing to compare it with, and
 *    undo takes the whole of it back.
 *  - A REWRITE PROPOSES. While the model thinks, the words it will replace
 *    are washed (the telegraph, as the CodeMirror writer draws it), and the
 *    selection is marked as a proposed deletion when the first words arrive, and the rewrite streams in after it as a
 *    proposed insertion sharing its id (`lib/document/suggestions.ts`), for
 *    the person to accept or reject. The original is never sent back through
 *    the model to be reproduced. Only text carries the marks: a mention or a
 *    line break inside the selection stays, whichever way they decide. In a
 *    code block, which holds no marks, the rewrite replaces the selection;
 *    across a code block's edge it is not written at all.
 *  - At the end, output that reads as markdown (a list, a heading, `**`) goes
 *    through the server's converter, and the blocks it makes replace only what
 *    this session wrote, in one transaction, still proposed on a rewrite. If
 *    the server cannot convert it, or anyone has changed those words since,
 *    the text stays as it streamed; on a rewrite it stays too when rejecting
 *    the converted blocks would not take them all back (a mention, a rule or
 *    a code block, which no mark can hold).
 *
 * Typing in the page or Escape stops it (`DocumentEditor`); what has arrived
 * by then stays, its marks whole.
 */

import type { Editor } from "@tiptap/core";
import { Slice, type Mark, type Node as PmNode, type ResolvedPos } from "@tiptap/pm/model";
import type { EditorState, Transaction } from "@tiptap/pm/state";
import { canSplit } from "@tiptap/pm/transform";
import { yUndoPluginKey } from "@tiptap/y-tiptap";
import type * as Y from "yjs";
import { toast } from "svelte-sonner";
import { convertMarkdown } from "$lib/api/client";
import { anchorAt, isBound, type Anchor } from "$lib/document/anchor";
import type { TreeAiDriver } from "$lib/document/editor";
import { clearAi, setAiCaret, setAiTelegraph, setAiTrail } from "$lib/document/presence";
import { DELETION, INSERTION, PROPOSAL_ATTR, withProposalRejected } from "$lib/document/suggestions";
import { htmlSlice, looksLikeMarkdown } from "$lib/document/paste";
import { contract, newId } from "$lib/document/schema";
import { aiSession } from "./aiSession.svelte";
import { abortAiSession } from "./aiCursorSession";
import { streamCompletion, type AiCompleteRequest, type AiIntent } from "./inlineComplete";
import { createOutputSanitizer } from "./sanitize";

const CONTEXT_CHARS = 1200;
const FLUSH_MS = 30;
const DONE_DISSOLVE_MS = 650;
/** How long a rewrite shows what it will replace before it asks the model: the CodeMirror writer's beat. */
const TELEGRAPH_MS = 380;
// Client-side backstop on top of the server's max_tokens, so a runaway
// completion can never fill the page.
const MAX_OUTPUT_CHARS = 8000;

/** What a session talks to; the defaults are the server's. Tests pass their own. */
export interface TreeAiDeps {
	stream: (req: AiCompleteRequest, signal: AbortSignal) => AsyncIterable<string>;
	convert: (markdown: string, signal?: AbortSignal) => Promise<{ html: string }>;
	flushMs: number;
	dissolveMs: number;
	telegraphMs: number;
}

const DEFAULTS: TreeAiDeps = {
	stream: streamCompletion,
	convert: convertMarkdown,
	flushMs: FLUSH_MS,
	dissolveMs: DONE_DISSOLVE_MS,
	telegraphMs: TELEGRAPH_MS,
};

export interface StartTreeAi {
	editor: Editor;
	intent: AiIntent;
	instruction: string;
	/** The page's real title, for the model's context. */
	pageTitle?: string;
}

/** The text a model reads for a stretch of the page: blocks apart, a mention as its label. */
function textOf(doc: PmNode, from: number, to: number, blockSeparator = "\n\n"): string {
	return doc.textBetween(from, to, blockSeparator, (leaf: PmNode) => {
		if (leaf.type.name === "mention") return String(leaf.attrs.label ?? "");
		if (leaf.type.name === "hardBreak") return "\n";
		return "";
	});
}

function inCode($pos: ResolvedPos): boolean {
	return !!$pos.parent.type.spec.code;
}

/** Text ranges in `from..to` that may carry `mark`, so a mark never lands on a mention. */
function markText(tr: Transaction, from: number, to: number, mark: Mark): void {
	const ranges: [number, number][] = [];
	tr.doc.nodesBetween(from, to, (node, pos, parent) => {
		if (!node.isText) return true;
		if (parent && parent.type.allowsMarkType(mark.type)) {
			const start = Math.max(from, pos);
			const end = Math.min(to, pos + node.nodeSize);
			if (start < end) ranges.push([start, end]);
		}
		return false;
	});
	for (const [start, end] of ranges) tr.addMark(start, end, mark);
}

/**
 * Put the converter's blocks in place of `from..to`. When the range is whole
 * blocks side by side, they are replaced whole, so a list or a heading stays
 * one; otherwise the blocks join the text around them, the words the writer
 * streamed there. (A paste instead splits the text around it,
 * `placeConverted`: it has no streamed words to stand in for.)
 */
function replaceConverted(tr: Transaction, from: number, to: number, html: string): void {
	const slice = htmlSlice(tr.doc.type.schema, html);
	const $from = tr.doc.resolve(from);
	const $to = tr.doc.resolve(to);
	const depth = $from.depth;
	const wholeBlocks =
		depth > 0 &&
		$to.depth === depth &&
		$from.parent.isTextblock &&
		$to.parent.isTextblock &&
		$from.parentOffset === 0 &&
		$to.parentOffset === $to.parent.content.size &&
		$from.node(depth - 1) === $to.node(depth - 1);
	if (wholeBlocks) {
		tr.replaceRange($from.before(), $to.after(), new Slice(slice.content, 0, 0));
	} else {
		tr.replaceRange(from, to, slice);
	}
}

/**
 * A transaction for the writer's words. Autolink stays off them: it would
 * read them joined to the text they follow ("tea." and a rewrite's "Coffee"
 * read as a domain) and link across both, and a rewrite's reject would leave
 * that link on the person's own words. The converter links what the output
 * writes as a link.
 */
function writerTr(state: EditorState): Transaction {
	return state.tr.setMeta("preventAutolink", true);
}

class TreeAiSession {
	private readonly editor: Editor;
	private readonly intent: AiIntent;
	private readonly instruction: string;
	private readonly pageTitle?: string;
	private readonly deps: TreeAiDeps;
	/** Whether the editor is bound to a shared document, which the writer's anchors need. */
	private readonly bound: boolean;

	private readonly controller = new AbortController();
	private aborted = false;

	/** Where the next words go: just after the last ones written. */
	private anchor: Anchor | null = null;
	/** Where this session's words start, once it has written some. */
	private start: Anchor | null = null;
	/** The selection a rewrite takes out, until the first words arrive. */
	private pending: { from: Anchor; to: Anchor } | null = null;
	/** The proposal a rewrite is writing, once it has marked the selection. */
	private proposal: string | null = null;
	/** Whether a rewrite writes in a block of its own now, not the one it began in. */
	private ownBlock = false;

	private isRewrite = false;
	private code = false;
	/** The output's last line break, waiting for words before it starts a block. */
	private breakPending = false;
	private buffer = "";
	private flushScheduled = false;
	private insertedChars = 0;
	/** The model's text as it arrived, for the converter. */
	private output = "";
	/** What this session put in the page, read back as `textOf` with "\n" between blocks. */
	private written = "";
	private sanitizer = createOutputSanitizer();

	private undo: { manager: Y.UndoManager; timeout: number } | null = null;

	constructor(o: StartTreeAi, deps: TreeAiDeps) {
		this.editor = o.editor;
		this.intent = o.intent;
		this.instruction = o.instruction;
		this.pageTitle = o.pageTitle;
		this.deps = deps;
		this.bound = isBound(o.editor.state);
	}

	writesTo(editor: Editor): boolean {
		return this.editor === editor;
	}

	/**
	 * Stop now. What has arrived lands and the undo step closes at once, not
	 * when the stream winds down: the keystroke that stopped the writer is
	 * the person's own step, never part of the writer's.
	 */
	abort(): void {
		if (this.aborted) return;
		this.aborted = true;
		this.controller.abort();
		this.landBuffer();
		this.releaseUndo();
	}

	private track(pos: number): Anchor | null {
		return this.bound ? anchorAt(this.editor.state, pos) : null;
	}

	/** Make everything this session writes one undo step, apart from what came before. */
	private holdUndo() {
		const manager = (yUndoPluginKey.getState(this.editor.state) as { undoManager?: Y.UndoManager } | undefined)
			?.undoManager;
		if (!manager) return;
		manager.stopCapturing();
		this.undo = { manager, timeout: manager.captureTimeout };
		manager.captureTimeout = Number.POSITIVE_INFINITY;
	}

	private releaseUndo() {
		if (!this.undo) return;
		this.undo.manager.captureTimeout = this.undo.timeout;
		this.undo.manager.stopCapturing();
		this.undo = null;
	}

	private live(): boolean {
		return !this.editor.isDestroyed;
	}

	async run(): Promise<void> {
		if (!this.bound) {
			console.error("[treeAiSession] the editor is bound to no shared document");
			return;
		}
		aiSession.set("thinking");
		this.holdUndo();

		const { state } = this.editor;
		const sel = state.selection;
		const doc = state.doc;
		this.isRewrite = this.intent === "rewrite" && !sel.empty;
		// A selection in one code block, which holds no marks, is replaced
		// there. One across a code block's edge is neither: the rewrite would
		// land in the code block, where nothing can be proposed, so it is
		// not written (`codeAt`, which the editor asks before offering it).
		this.code = inCode(sel.$from) && sel.$from.sameParent(sel.$to);
		if (this.isRewrite && !this.code && (inCode(sel.$from) || inCode(sel.$to))) {
			this.cleanup();
			return;
		}

		let selection: string | undefined;
		let before: string;
		let after: string;
		if (this.isRewrite) {
			selection = textOf(doc, sel.from, sel.to);
			before = textOf(doc, 0, sel.from).slice(-CONTEXT_CHARS);
			after = textOf(doc, sel.to, doc.content.size).slice(0, CONTEXT_CHARS);
			this.pending = { from: this.track(sel.from)!, to: this.track(sel.to)! };
			this.anchor = this.track(sel.to);
			// The rewrite lands after the selection; a selection growing over it
			// would raise the format bar over words still arriving. The words it
			// will replace stay washed until the first words mark them (`write`).
			this.editor.commands.setTextSelection(sel.to);
			setAiCaret(this.editor, { pos: sel.to, phase: "active" });
			aiSession.set("telegraphing");
			setAiTelegraph(this.editor, { from: sel.from, to: sel.to });
			await new Promise((r) => setTimeout(r, this.deps.telegraphMs));
			if (this.aborted) {
				this.cleanup();
				return;
			}
		} else {
			before = textOf(doc, 0, sel.head).slice(-CONTEXT_CHARS);
			after = textOf(doc, sel.head, doc.content.size).slice(0, CONTEXT_CHARS);
			this.anchor = this.track(sel.head);
			setAiCaret(this.editor, { pos: sel.head, phase: "active" });
		}

		try {
			aiSession.set("streaming");
			const stream = this.deps.stream(
				{
					intent: this.intent,
					instruction: this.instruction,
					selection,
					context_before: before,
					context_after: after,
					page_title: this.pageTitle?.trim() || undefined,
				},
				this.controller.signal,
			);

			for await (const chunk of stream) {
				if (this.aborted) break;
				const safe = this.sanitizer.push(chunk);
				if (safe) {
					this.buffer += safe;
					this.scheduleFlush();
				}
			}

			if (this.aborted) {
				this.landBuffer();
				this.notifyInterrupted();
				this.cleanup();
				return;
			}

			const tail = this.sanitizer.flush();
			if (tail) this.buffer += tail;
			this.landBuffer();
			await this.convertOutput();

			const end = this.anchor?.resolve();
			if (end != null && this.live()) setAiCaret(this.editor, { pos: end, phase: "done" });
			aiSession.set("done");
			await new Promise((r) => setTimeout(r, this.deps.dissolveMs));
			this.cleanup();
		} catch (err) {
			// Whatever arrived lands: every write is whole, so the page holds
			// no half-made proposal either way.
			this.landBuffer();
			if (this.aborted || (err instanceof DOMException && err.name === "AbortError")) {
				this.notifyInterrupted();
				this.cleanup();
			} else {
				console.error("AI writer failed:", err);
				aiSession.set("error", err instanceof Error ? err.message : "AI error");
				if (this.isRewrite && this.proposal) {
					toast.error("AI edit failed", { description: this.recoveryHint() });
				}
				this.cleanup(false);
			}
		}
	}

	private scheduleFlush() {
		if (this.flushScheduled) return;
		this.flushScheduled = true;
		setTimeout(() => {
			this.flushScheduled = false;
			if (this.aborted) return;
			const text = this.buffer;
			this.buffer = "";
			this.write(text);
		}, this.deps.flushMs);
	}

	/** Write what is buffered now, aborted or not: words that arrived are kept. */
	private landBuffer() {
		const text = this.buffer;
		this.buffer = "";
		if (text) this.write(text);
	}

	/**
	 * Start a new block at `pos`, returning where the next words go there; null
	 * when the block cannot be split. A block with nothing before `pos` is not
	 * split: that would leave an empty block behind.
	 */
	private split(tr: Transaction, pos: number): number | null {
		const $pos = tr.doc.resolve(pos);
		if (!$pos.parent.isTextblock) return null;
		if ($pos.parentOffset === 0) return pos;
		const { schema } = tr.doc.type;
		// After a heading comes a paragraph, as Enter makes one.
		const after = $pos.parent.type.name === "heading" ? [{ type: schema.nodes.paragraph }] : undefined;
		if (!canSplit(tr.doc, pos, 1, after)) return null;
		// `pos` is in the document as this transaction has made it so far.
		const steps = tr.steps.length;
		tr.split(pos, 1, after);
		return tr.mapping.slice(steps).map(pos, 1);
	}

	/**
	 * A rewrite's first line break, in the block it began in: that block is
	 * not split, so a reject leaves it whole. What follows `pos` in it is
	 * proposed out of it and proposed again in a new block after it, and the
	 * next words go at the start of that block. Null when the tail holds
	 * anything a mark cannot (a mention, a line break, another proposal) or
	 * no block can follow here; `pos` when nothing comes before it.
	 */
	private moveTail(tr: Transaction, pos: number, id: string): number | null {
		const $pos = tr.doc.resolve(pos);
		const block = $pos.parent;
		if (!block.isTextblock) return null;
		if ($pos.parentOffset === 0) return pos;
		const { schema } = tr.doc.type;
		const del = schema.marks[DELETION].create({ [PROPOSAL_ATTR]: id });
		const ins = schema.marks[INSERTION].create({ [PROPOSAL_ATTR]: id });
		if (!block.type.allowsMarkType(ins.type)) return null;
		const tail: PmNode[] = [];
		let markable = true;
		block.content.cut($pos.parentOffset).forEach((node) => {
			if (!node.isText || node.marks.some((m) => m.type === del.type || m.type === ins.type)) markable = false;
			else tail.push(node.mark(ins.addToSet(node.marks)));
		});
		if (!markable) return null;
		// After a heading comes a paragraph, as Enter makes one.
		const type = block.type.name === "heading" ? schema.nodes.paragraph : block.type;
		const attrs = type === block.type ? { ...block.attrs, [contract.id.attr]: null } : null;
		const after = $pos.after();
		const $after = tr.doc.resolve(after);
		if (!$after.parent.canReplaceWith($after.index(), $after.index(), type)) return null;
		if ($pos.end() > pos) tr.addMark(pos, $pos.end(), del);
		tr.insert(after, type.create(attrs, tail));
		return after + 1;
	}

	/** One chunk, as one transaction: the selection marked first if it is the first. */
	private write(raw: string): void {
		if (!raw || !this.live()) return;
		const at = this.anchor?.resolve();
		if (at == null) {
			// The block it was writing in is gone (deleted on another device).
			this.abort();
			return;
		}
		const { state } = this.editor;
		const { schema } = state;
		const tr = writerTr(state);
		let pos = at;
		// The first words: the telegraph comes off as the selection is marked.
		const first = this.pending !== null;

		if (this.pending) {
			const from = this.pending.from.resolve();
			const to = this.pending.to.resolve();
			this.pending = null;
			if (from != null && to != null && from < to) {
				if (this.code) {
					tr.delete(from, to);
					pos = tr.mapping.map(pos, -1);
				} else {
					this.proposal = newId();
					markText(tr, from, to, schema.marks[DELETION].create({ [PROPOSAL_ATTR]: this.proposal }));
				}
			}
		}

		const text = raw.replace(/\r\n?/g, "\n");
		const marks = this.proposal ? [schema.marks[INSERTION].create({ [PROPOSAL_ATTR]: this.proposal })] : [];
		const startOfWrite = pos;
		// Where this session's first words go, when they are in this chunk.
		let firstAt: number | null = null;
		const begun = () => this.start !== null || firstAt !== null;
		let wrote = "";
		const runs = this.code ? [text] : text.split(/\n+/);
		for (let i = 0; i < runs.length; i++) {
			// A line break starts a new block when words follow it, so the
			// output's last line break leaves no empty block behind.
			if (i > 0) this.breakPending = true;
			const run = runs[i];
			if (!run) continue;
			if (this.breakPending) {
				this.breakPending = false;
				const fromOwn = this.proposal === null || this.ownBlock;
				const next = fromOwn ? this.split(tr, pos) : this.moveTail(tr, pos, this.proposal!);
				if (next !== null && next !== pos && this.proposal) this.ownBlock = true;
				if (next === null) {
					if (begun()) {
						tr.insert(pos, schema.text(" ", marks));
						pos += 1;
						wrote += " ";
					}
				} else if (next !== pos) {
					pos = next;
					if (begun()) wrote += "\n";
				}
			}
			if (!begun()) firstAt = pos;
			tr.insert(pos, schema.text(run, marks));
			pos += run.length;
			wrote += run;
		}
		// The converter reads the model's text whole, line breaks that start no
		// block included.
		this.output += raw;
		this.insertedChars += raw.length;
		if (tr.docChanged) this.editor.view.dispatch(tr);
		if (first) setAiTelegraph(this.editor, null);
		if (!tr.docChanged) return;

		if (firstAt !== null) this.start = this.track(firstAt);
		this.written += wrote;
		this.anchor = this.track(pos);
		setAiTrail(this.editor, { from: startOfWrite, to: pos });
		setAiCaret(this.editor, { pos, phase: "active" });
		if (this.insertedChars >= MAX_OUTPUT_CHARS) this.abort();
	}

	/** This session's words, where they are now, if they are still exactly as written. */
	private ownRange(): { from: number; to: number } | null {
		if (!this.live() || !this.start || !this.anchor) return null;
		const from = this.start.resolve();
		const to = this.anchor.resolve();
		if (from == null || to == null || from >= to) return null;
		const doc = this.editor.state.doc;
		return textOf(doc, from, to, "\n") === this.written ? { from, to } : null;
	}

	/** Replace what streamed with the blocks its markdown makes, when it reads as markdown. */
	private async convertOutput(): Promise<void> {
		if (this.code || !this.written || !looksLikeMarkdown(this.output, { inline: true })) return;
		if (!this.ownRange()) return;
		let html: string;
		try {
			({ html } = await this.deps.convert(this.output, this.controller.signal));
		} catch {
			// The text stays as it streamed.
			return;
		}
		if (this.aborted) return;
		const own = this.ownRange();
		if (!own) return;
		// Spaces around the words stay as they are: the converter trims a
		// paragraph, and " **bold**" after a word must keep its space.
		const lead = /^[ \t]*/.exec(this.written)![0].length;
		const trail = /[ \t]*$/.exec(this.written)![0].length;
		const range = { from: own.from + lead, to: own.to - trail };
		if (range.from >= range.to) return;
		const { state } = this.editor;
		const tr = writerTr(state);
		replaceConverted(tr, range.from, range.to, html);
		const from = tr.mapping.map(range.from, -1);
		const to = tr.mapping.map(range.to, 1);
		if (this.proposal) {
			markText(tr, from, to, state.schema.marks[INSERTION].create({ [PROPOSAL_ATTR]: this.proposal }));
			// Rejected, the converted page must read as the streamed one does
			// rejected: a node no mark can hold, or a block the page had split,
			// would outlive the reject.
			const rejected = withProposalRejected(state.doc, this.proposal);
			if (!withProposalRejected(tr.doc, this.proposal).eq(rejected)) return;
		}
		const end = tr.mapping.map(own.to, 1);
		this.editor.view.dispatch(tr);
		this.anchor = this.track(Math.min(end, this.editor.state.doc.content.size));
	}

	/** How to get rid of what the writer did, phrased for what it did. */
	private recoveryHint(): string {
		return this.proposal ? "Reject the suggestion to keep your text as it was." : "Press ⌘Z to undo.";
	}

	/** A rewrite that began writing was interrupted: say how to recover. */
	private notifyInterrupted() {
		if (this.isRewrite && this.proposal) {
			toast("AI edit interrupted", { description: this.recoveryHint() });
		}
	}

	/** Take the caret and trail down. `resetStatus` false keeps an error showing. */
	private cleanup(resetStatus = true) {
		this.releaseUndo();
		if (this.live()) clearAi(this.editor);
		if (resetStatus) aiSession.reset();
	}
}

/** The writer for one app: one session at a time across every page. */
export function createTreeAiDriver(
	deps: Partial<TreeAiDeps> = {},
): TreeAiDriver & { abort(): void; whenIdle(): Promise<void> } {
	const resolved: TreeAiDeps = { ...DEFAULTS, ...deps };
	let current: TreeAiSession | null = null;
	let running: Promise<void> = Promise.resolve();
	return {
		start(o) {
			// The CodeMirror editor's writer, if one is running elsewhere.
			abortAiSession();
			current?.abort();
			const session = new TreeAiSession(o, resolved);
			current = session;
			running = session.run().finally(() => {
				if (current === session) current = null;
			});
		},
		abortIn(editor) {
			if (current?.writesTo(editor)) current.abort();
		},
		/** Stop the running session wherever it writes: the status bar's Stop. */
		abort() {
			current?.abort();
		},
		isActive() {
			return current !== null;
		},
		/** Resolves when the running session, if any, has finished: for tests. */
		whenIdle() {
			return running;
		},
	};
}

export const treeAiDriver = createTreeAiDriver();
