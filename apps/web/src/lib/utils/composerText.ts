/**
 * Serialize the chat composer's contenteditable DOM to the text the user
 * typed.
 *
 * The composer is a `contenteditable` div, not a textarea, because @-mention
 * pills are elements. That means the browser — WebKit on every Virtues
 * client — represents line structure as DOM, not as `\n`:
 *
 *   - Shift+Enter wraps each new line in a `<div>`; the first line stays a
 *     bare text node, so the tree reads `line1<div>line2</div>`.
 *   - An empty line is `<div><br></div>`.
 *   - A `<br>` at the very end of a block is WebKit's caret placeholder, not
 *     a line break the user typed.
 *   - Pasted multi-line text (through `execCommand("insertText")`) comes out
 *     in the same block shapes.
 *   - Trailing and doubled spaces are stored as U+00A0 so they survive
 *     whitespace collapsing.
 *
 * Reading `textContent` — or walking only text nodes — drops every one of
 * those, which is how three typed bullet lines were reaching the model and
 * the thread as one run-on line (VIR-333). This walker treats blocks as
 * lines and `<br>` as a line break, and hands back plain text with real
 * newlines. Edge trimming is the caller's decision.
 */

/** A mention pill in the composer. Serialized as a markdown link so the
 *  user-message renderer and the model both see `[Name](/person/id)`. */
export interface PillSerializer {
	/** Class that marks a pill element. */
	className: string;
	/** Text to emit for the pill. Return null to fall back to its text. */
	serialize: (el: HTMLElement) => string | null;
}

const BLOCK_TAGS = new Set([
	"DIV",
	"P",
	"LI",
	"UL",
	"OL",
	"BLOCKQUOTE",
	"PRE",
	"H1",
	"H2",
	"H3",
	"H4",
	"H5",
	"H6",
	"SECTION",
	"ARTICLE",
]);

export function serializeComposer(root: Node, pill?: PillSerializer): string {
	let out = "";
	// True once anything — text or a block — has been seen. A block that is
	// not the first thing in the composer starts a new line, even when the
	// previous line was empty and emitted nothing; that is what keeps a
	// blank line between paragraphs from collapsing.
	let sawContent = false;

	const walk = (node: Node, parent: Node) => {
		if (node.nodeType === Node.TEXT_NODE) {
			const text = node.textContent ?? "";
			if (text.length > 0) {
				out += text;
				sawContent = true;
			}
			return;
		}
		if (node.nodeType !== Node.ELEMENT_NODE) return;
		const el = node as HTMLElement;

		// Pill icons are inline SVG; a <title> inside one is not typed text.
		if (el.tagName.toLowerCase() === "svg") return;

		if (el.tagName === "BR") {
			// WebKit parks a <br> at the end of a block (and at the end of the
			// root) so the caret has somewhere to sit. It is not a line break
			// the user typed; the block boundary already supplies that.
			if (el === parent.lastChild) return;
			out += "\n";
			sawContent = true;
			return;
		}

		if (pill && el.classList.contains(pill.className)) {
			const s = pill.serialize(el) ?? el.textContent ?? "";
			out += s;
			sawContent = true;
			return;
		}

		if (el !== root && BLOCK_TAGS.has(el.tagName)) {
			if (sawContent) out += "\n";
			sawContent = true;
		}

		for (const child of Array.from(el.childNodes)) walk(child, el);
	};

	walk(root, root);

	return out.replace(/ /g, " ");
}
