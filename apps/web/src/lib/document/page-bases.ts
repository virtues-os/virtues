/**
 * The stock extensions for what only a page holds: headings, horizontal
 * rules, images (with the typed `![name](src)` that makes any media block),
 * to-dos, tables and underline. schema.ts takes them as `PageBases`, so the
 * chat composer, which loads with the app's first paint and holds only a
 * message (`MESSAGE` in message.ts), never loads this module or the
 * libraries it imports. The page editor and its preview do.
 */
import { InputRule, getExtensionField, getSchema, type Extensions, type NodeConfig } from '@tiptap/core';
import type { NodeType, Schema } from '@tiptap/pm/model';
import { TextSelection } from '@tiptap/pm/state';
import { findWrapping } from '@tiptap/pm/transform';
import Heading, { type Level } from '@tiptap/extension-heading';
import HorizontalRule from '@tiptap/extension-horizontal-rule';
import Image, { inputRegex as imageInputRegex } from '@tiptap/extension-image';
import { TaskItem, TaskList, inputRegex as taskInputRegex } from '@tiptap/extension-list';
import { Table, TableCell, TableHeader, TableRow } from '@tiptap/extension-table';
import Underline from '@tiptap/extension-underline';
import { altWidth, kindOfLink, type MediaKind } from './media-kind';
import { insertBlockAt } from './place';
import { contract, contractExtensions, contractUrl, joinListsAround, type Extras, type PageBases } from './schema';

/** Heading levels as the contract lists them, for the stock keymap and input rules. */
function headingLevels(): Level[] {
	const levels = contract.nodes.heading?.attrs?.level?.enum;
	if (!levels?.length) throw new Error('contract: heading.level has no enum');
	return levels as Level[];
}


/**
 * What a typed `![name](src)` makes of a `kind`: the block `kindOfLink`
 * reads it as (an image, audio, video or a file), as the server's
 * converter reads it, a `|600` after the name
 * being an image's width (`altWidth`); nothing when it is another kind, or
 * the contract refuses its `src`. The match is the `![name](src)` alone,
 * not the space before it.
 */
function typedMedia(kind: MediaKind) {
	return (text: string) => {
		const m = imageInputRegex.exec(text);
		const src = m ? contractUrl(contract.nodes[kind]?.attrs?.src, m[3]) : null;
		if (!m || src === null) return null;
		const { alt, width } = altWidth(m[2]);
		if (kindOfLink(src, alt) !== kind) return null;
		const data = kind === 'image' ? { src, alt: alt || null, width } : { src, name: alt || null };
		return { index: m.index + m[0].lastIndexOf(m[1]), text: m[1], data };
	};
}

/**
 * The block a typed `![name](src)` makes, in place of what was typed: in
 * place of the line when that was all it held, else after its words or
 * between them. The caret goes on in the text after the block, on a new
 * line when nothing follows it there.
 */
function typedBlock(type: NodeType, kind: MediaKind): InputRule {
	return new InputRule({
		find: typedMedia(kind),
		handler: ({ state, range, match }) => {
			const { tr } = state;
			tr.delete(range.from, range.to);
			const end = insertBlockAt(tr, range.from, type.create(match.data ?? {}));
			const $end = tr.doc.resolve(end);
			const paragraph = state.schema.nodes.paragraph;
			if ($end.nodeAfter?.isTextblock) {
				tr.setSelection(TextSelection.create(tr.doc, end + 1));
			} else if ($end.parent.canReplaceWith($end.index(), $end.index(), paragraph)) {
				tr.insert(end, paragraph.create());
				tr.setSelection(TextSelection.create(tr.doc, end + 1));
			}
			tr.scrollIntoView();
		},
	});
}

/**
 * The stock image, whose `setImage` and `![alt](src)` input rule take any
 * `src`: here a `src` the contract refuses (`data:`, `javascript:`) inserts
 * nothing, and the typed text stays text. The typed form makes the block
 * the address is (`typedMedia`): audio for an `.mp3`, a file for a `.pdf`.
 */
const ContractImage = Image.extend({
	addCommands() {
		return {
			setImage:
				(options) =>
				({ commands }) => {
					const src = contractUrl(contract.nodes.image?.attrs?.src, options.src);
					if (src === null) return false;
					return commands.insertContent({ type: this.name, attrs: { ...options, src } });
				},
		};
	},
	addInputRules() {
		const kinds: MediaKind[] = ['image', 'audio', 'video', 'file'];
		return kinds.flatMap((kind) => {
			const type = this.editor.schema.nodes[kind];
			return type ? [typedBlock(type, kind)] : [];
		});
	},
});

/**
 * Tiptap's to-do, its box ticked without taking the keyboard. The stock
 * box focuses the editor before it writes the tick, which on a phone raises
 * the keyboard over half the screen for someone only reading the page, and
 * puts the caret back where it last was. Here the tick is written as it
 * is, ahead of the stock handler, and the editor takes the focus back only
 * when a pointer ticked the box while the editor had it; a tick by Space
 * leaves the focus on the box. A page that is read only is the stock
 * handler's: it puts the box back.
 *
 * Keys pressed on the box are the box's, as a widget's controls keep theirs
 * (`nodeviews.svelte.ts`): the editor would read them as typed where the
 * caret is, splitting a paragraph elsewhere on Enter or nesting another
 * item on Tab. ⌘Enter ticks the to-do the caret is in, from the text
 * (`TodoKeys` in commands.ts).
 */
const QuietTaskItem = TaskItem.extend({
	// Tiptap's typed `[ ] `, joining the to-do list it makes to one just
	// above or below. The stock rule joins only an item of the type it
	// wraps in, and the node beside a new to-do is a list.
	addInputRules() {
		return [
			new InputRule({
				find: taskInputRegex,
				handler: ({ state, range, match }) => {
					const tr = state.tr.delete(range.from, range.to);
					const block = tr.doc.resolve(range.from).blockRange();
					const wrapping = block && findWrapping(block, this.type, { checked: match[match.length - 1] === 'x' });
					if (!block || !wrapping) return null;
					tr.wrap(block, wrapping);
					joinListsAround(tr, block.start);
				},
			}),
		];
	},
	addNodeView() {
		// The stock view itself, not `this.parent`: the contract extends this
		// extension again, and Tiptap hands that level this one as its parent,
		// so `this.parent` there is this wrapper, its listeners added twice.
		const stock = getExtensionField<NonNullable<NodeConfig['addNodeView']>>(TaskItem, 'addNodeView', {
			name: this.name,
			options: this.options,
			storage: this.storage,
			editor: this.editor,
			type: this.type,
		})();
		if (!stock) return null;
		return (props) => {
			const rendered = stock(props);
			const dom = rendered.dom as HTMLElement;
			const box = dom.querySelector('input[type="checkbox"]');
			const label = box?.parentElement;
			if (!(box instanceof HTMLInputElement) || !label) return rendered;
			const { editor } = props;
			// Whether the editor had the focus when a pointer went down on the
			// box: set by that pointer, and spent by the tick it makes.
			let hadFocus = false;
			label.addEventListener('pointerdown', () => (hadFocus = editor.view.hasFocus()), true);
			label.addEventListener('keydown', () => (hadFocus = false), true);
			// On the way down to the box, so the stock handler never runs.
			label.addEventListener(
				'change',
				(event) => {
					if (event.target !== box || !editor.isEditable) return;
					event.stopPropagation();
					const refocus = hadFocus;
					hadFocus = false;
					const pos = typeof props.getPos === 'function' ? props.getPos() : undefined;
					const node = typeof pos === 'number' ? editor.state.doc.nodeAt(pos) : null;
					if (typeof pos !== 'number' || !node) return;
					editor.view.dispatch(editor.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, checked: box.checked }));
					if (refocus) editor.view.focus();
				},
				true,
			);
			rendered.stopEvent = (event: Event) => label.contains(event.target as globalThis.Node | null);
			return rendered;
		};
	},
});

/** The stock extension each node only a page holds extends. */
function pageNode(name: string) {
	switch (name) {
		case 'heading':
			return Heading.configure({ levels: headingLevels() });
		case 'taskList':
			return TaskList;
		case 'taskItem':
			return QuietTaskItem.configure({ nested: true });
		case 'horizontalRule':
			return HorizontalRule;
		case 'image':
			return ContractImage;
		case 'table':
			return Table.configure({ resizable: false });
		case 'tableRow':
			return TableRow;
		case 'tableHeader':
			return TableHeader;
		case 'tableCell':
			return TableCell;
		default:
			return undefined;
	}
}

export const PAGE_BASES: PageBases = {
	node: pageNode,
	mark: (name) => (name === 'underline' ? Underline : undefined),
};

/** A page editor's extensions: the whole contract with its page bases. */
export function pageExtensions(extras: Extras = {}): Extensions {
	return contractExtensions(extras, undefined, PAGE_BASES);
}

/** A page's ProseMirror schema alone, for parsing and serializing without an editor. */
export function contractSchema(): Schema {
	return getSchema(pageExtensions());
}
