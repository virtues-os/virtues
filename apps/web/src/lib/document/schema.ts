/**
 * The page editor's schema, built from the document contract
 * (`crates/virtues-document/contract.json`, imported as `$contract`). The Rust
 * crate reads the same file to check the model's HTML, write the Yjs tree and
 * render canonical HTML, so the two sides cannot drift without
 * `conformance.test.ts` noticing.
 *
 * Tiptap's extensions supply behaviour: keymaps, input rules, commands, node
 * views. The contract supplies every name, content expression, attribute and
 * HTML mapping, and overrides whatever the stock extension declares for them.
 *
 * No Svelte here, so tests and non-editor code can build the schema.
 */

import {
	Extension,
	Mark,
	Node,
	getSchema,
	nodeInputRule,
	type Attributes,
	type Extensions,
	type MarkConfig,
	type NodeConfig,
} from '@tiptap/core';
import type {
	DOMOutputSpec,
	Fragment,
	Mark as PmMark,
	Node as PmNode,
	ParseRule,
	Schema,
	TagParseRule,
} from '@tiptap/pm/model';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import {
	AddMarkStep,
	AddNodeMarkStep,
	AttrStep,
	ReplaceAroundStep,
	ReplaceStep,
	type Step,
} from '@tiptap/pm/transform';
import { ySyncPluginKey } from '@tiptap/y-tiptap';
import Blockquote from '@tiptap/extension-blockquote';
import Bold from '@tiptap/extension-bold';
import Code from '@tiptap/extension-code';
import CodeBlock from '@tiptap/extension-code-block';
import Document from '@tiptap/extension-document';
import HardBreak from '@tiptap/extension-hard-break';
import Heading, { type Level } from '@tiptap/extension-heading';
import Highlight from '@tiptap/extension-highlight';
import HorizontalRule from '@tiptap/extension-horizontal-rule';
import Image, { inputRegex as imageInputRegex } from '@tiptap/extension-image';
import Italic from '@tiptap/extension-italic';
import Link from '@tiptap/extension-link';
import { BulletList, ListItem, OrderedList, TaskItem, TaskList } from '@tiptap/extension-list';
import Paragraph from '@tiptap/extension-paragraph';
import Strike from '@tiptap/extension-strike';
import { Table, TableCell, TableHeader, TableRow } from '@tiptap/extension-table';
import Text from '@tiptap/extension-text';
import Underline from '@tiptap/extension-underline';
import contractJson from '$contract';

export type AttrSpec = {
	/** `int`: a safe integer. `route`: a ref on the box, `/kind/id` (`isRoute`). */
	type: 'int' | 'string' | 'bool' | 'url' | 'route';
	default?: unknown;
	required?: boolean;
	enum?: unknown[];
	/** The least an `int` may be. */
	min?: number;
	/** The most an `int` may be. */
	max?: number;
	/** The HTML attribute that carries it. None: set only by a rule's `set`. */
	html?: string;
	/** Read from the first child's class with this prefix (`<pre><code class="language-rust">`). */
	fromChildClass?: string;
};

export type HtmlRule = {
	tag: string;
	/** Attribute values this tag implies (`h2` sets level 2). */
	set?: Record<string, unknown>;
	/** HTML attributes the element must carry for this rule to apply. */
	match?: Record<string, string>;
	/** Wrapper tags the server's ingest steps through. The browser's parser does so for any unknown tag. */
	transparent?: string[];
	/** Accepted on the way in, never written. */
	parseOnly?: boolean;
	/** A tag rendered between this element and its content (`table` > `tbody`). */
	wrap?: string;
};

export type NodeSpec = {
	group?: string;
	content?: string;
	marks?: string;
	code?: boolean;
	atom?: boolean;
	inline?: boolean;
	/** Carries a block id. */
	id?: boolean;
	attrs?: Record<string, AttrSpec>;
	html?: HtmlRule[];
};

export type MarkSpec = {
	attrs?: Record<string, AttrSpec>;
	html?: HtmlRule[];
	excludes?: string;
};

export type Contract = {
	version: number;
	/** The Yjs XML fragment the document lives in. */
	fragment: string;
	id: { attr: string; html: string };
	/** Schemes a `url` attribute may use, lowercase. A URL without one is a path on the box. */
	urlSchemes: string[];
	nodes: Record<string, NodeSpec>;
	/** In nesting order: the first mark is outermost in HTML. */
	marks: Record<string, MarkSpec>;
};

export const contract = contractJson as unknown as Contract;

/** Heading levels as the contract lists them, for the stock keymap and input rules. */
function headingLevels(): Level[] {
	const levels = contract.nodes.heading?.attrs?.level?.enum;
	if (!levels?.length) throw new Error('contract: heading.level has no enum');
	return levels as Level[];
}

/** A URL attribute's value as the server takes it, or null when it refuses it. */
function contractUrl(spec: AttrSpec | undefined, raw: unknown): string | null {
	if (!spec || typeof raw !== 'string') return null;
	const value = typed(spec, raw);
	return typeof value === 'string' ? value : null;
}

/**
 * The stock image, whose `setImage` and `![alt](src)` input rule take any
 * `src`: here a `src` the contract refuses (`data:`, `javascript:`) inserts
 * nothing, and the typed text stays text.
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
		return [
			nodeInputRule({
				find: (text) => {
					const m = imageInputRegex.exec(text);
					const src = m ? contractUrl(contract.nodes.image?.attrs?.src, m[3]) : null;
					if (!m || src === null) return null;
					return { index: m.index, text: m[0], replaceWith: m[1], data: { src, alt: m[2] } };
				},
				type: this.type,
				getAttributes: (match) => match.data ?? {},
			}),
		];
	},
});

/** Whether a link may point at `url`: the contract's schemes, not the stock list. */
function linkAllowed(url: string): boolean {
	return contractUrl(contract.marks.link?.attrs?.href, url) !== null;
}

const stockShouldAutoLink = Link.options.shouldAutoLink;

/** The stock extension each contract node extends. Others are created from the contract alone. */
function nodeBase(name: string): Node | undefined {
	switch (name) {
		case 'doc':
			return Document;
		case 'paragraph':
			return Paragraph;
		case 'text':
			return Text;
		case 'heading':
			return Heading.configure({ levels: headingLevels() });
		case 'blockquote':
			return Blockquote;
		case 'bulletList':
			return BulletList;
		case 'orderedList':
			return OrderedList;
		case 'listItem':
			return ListItem;
		case 'taskList':
			return TaskList;
		case 'taskItem':
			return TaskItem.configure({ nested: true });
		case 'codeBlock':
			return CodeBlock;
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
		case 'hardBreak':
			return HardBreak;
		default:
			return undefined;
	}
}

function markBase(name: string): Mark | undefined {
	switch (name) {
		case 'link':
			return Link.configure({
				openOnClick: false,
				autolink: true,
				// The stock check allows schemes the server refuses (ftp:, sms:,
				// xmpp:); setLink, toggleLink, autolink and pasted URLs ask this.
				isAllowedUri: (url) => linkAllowed(url),
				// A URL pasted over selected text asks only this.
				shouldAutoLink: (url) => linkAllowed(url) && stockShouldAutoLink(url),
			});
		case 'bold':
			return Bold;
		case 'italic':
			return Italic;
		case 'underline':
			return Underline;
		case 'strike':
			return Strike;
		case 'highlight':
			return Highlight;
		case 'code':
			return Code;
		default:
			return undefined;
	}
}

function defaultOf(spec: AttrSpec): unknown {
	return spec.default === undefined ? null : spec.default;
}

/**
 * A URL as a browser reads it before looking at its scheme (the WHATWG URL
 * parser): every ASCII tab and newline removed, wherever it is, and C0
 * controls and spaces trimmed from both ends. Checked any other way,
 * `java&#9;script:` and `&#1;javascript:` pass a prefix test and still run as
 * `javascript:` when followed. The server's `normalize_url` is the same.
 */
export function normalizeUrl(raw: string): string {
	const s = raw.replace(/[\t\n\r]/g, '');
	let start = 0;
	let end = s.length;
	while (start < end && s.charCodeAt(start) <= 0x20) start++;
	while (end > start && s.charCodeAt(end - 1) <= 0x20) end--;
	return s.slice(start, end);
}

/** The scheme of a normalized URL, lowercased; null for a path. */
export function urlScheme(url: string): string | null {
	const m = /^([A-Za-z][A-Za-z0-9+.-]*):/.exec(url);
	return m ? m[1].toLowerCase() : null;
}

/**
 * Whether `route` is a ref to something on the box, `/kind/id`, as the
 * server reads one (`is_route` in the crate): a kind of letters, digits, `_`
 * and `-`, so it is never read as another site, and an id before any `?` or
 * `#`.
 */
export function isRoute(route: string): boolean {
	const m = /^\/([A-Za-z0-9_-]+)\/([^?#]*)/.exec(route);
	return m !== null && m[2].length > 0;
}

/** Whether an `int` attribute's value is in its range: safe, and within `min` and `max`. */
function inRange(spec: AttrSpec, n: number): boolean {
	return (
		Number.isSafeInteger(n) &&
		(spec.min === undefined || n >= spec.min) &&
		(spec.max === undefined || n <= spec.max)
	);
}

/**
 * One HTML attribute value as the contract types it, or null when it is
 * absent or the server's ingest would refuse it.
 */
export function typed(spec: AttrSpec, raw: string | null): unknown {
	if (raw === null) return null;
	let value: unknown;
	switch (spec.type) {
		case 'int': {
			const s = raw.trim();
			if (!/^[+-]?\d+$/.test(s)) return null;
			const n = Number(s);
			if (!inRange(spec, n)) return null;
			value = n;
			break;
		}
		case 'bool': {
			const s = raw.trim();
			if (s === '' || s === 'true') value = true;
			else if (s === 'false') value = false;
			else return null;
			break;
		}
		case 'url': {
			const url = normalizeUrl(raw);
			const scheme = urlScheme(url);
			if (scheme !== null && !contract.urlSchemes.includes(scheme)) return null;
			value = url;
			break;
		}
		case 'route':
			if (!isRoute(raw)) return null;
			value = raw;
			break;
		case 'string':
			value = raw;
			break;
	}
	if (spec.enum && !spec.enum.includes(value)) return null;
	return value;
}

/**
 * Whether a value an editor holds fits the contract's attribute, as the
 * server's check decides (`AttrSpec::check` in the crate): null only where
 * the attribute is not required, the right type, a safe integer within
 * `min` and `max`, a URL with a scheme a page may hold, a route on the box,
 * a value in `enum`.
 */
export function fits(spec: AttrSpec, value: unknown): boolean {
	if (value === null || value === undefined) return !spec.required;
	switch (spec.type) {
		case 'int':
			if (typeof value !== 'number' || !inRange(spec, value)) return false;
			break;
		case 'route':
			if (typeof value !== 'string' || !isRoute(value)) return false;
			break;
		case 'bool':
			if (typeof value !== 'boolean') return false;
			break;
		case 'string':
			if (typeof value !== 'string') return false;
			break;
		case 'url': {
			if (typeof value !== 'string') return false;
			const scheme = urlScheme(normalizeUrl(value));
			if (scheme !== null && !contract.urlSchemes.includes(scheme)) return false;
			break;
		}
	}
	return !spec.enum || spec.enum.includes(value);
}

function markFits(mark: PmMark): boolean {
	const attrs = contract.marks[mark.type.name]?.attrs ?? {};
	return Object.entries(attrs).every(([key, spec]) => fits(spec, mark.attrs[key]));
}

function nodeFits(node: PmNode): boolean {
	const attrs = contract.nodes[node.type.name]?.attrs ?? {};
	return (
		Object.entries(attrs).every(([key, spec]) => fits(spec, node.attrs[key])) &&
		node.marks.every(markFits)
	);
}

function fragmentFits(fragment: Fragment): boolean {
	let ok = true;
	fragment.descendants((node) => {
		ok = ok && nodeFits(node);
		return ok;
	});
	return ok;
}

/** Whether what `step` writes fits the contract. `before` is the document it applies to. */
function stepFits(step: Step, before: PmNode): boolean {
	if (step instanceof ReplaceStep || step instanceof ReplaceAroundStep) return fragmentFits(step.slice.content);
	if (step instanceof AddMarkStep || step instanceof AddNodeMarkStep) return markFits(step.mark);
	if (step instanceof AttrStep) {
		const node = before.nodeAt(step.pos);
		const spec = node ? contract.nodes[node.type.name]?.attrs?.[step.attr] : undefined;
		return !spec || fits(spec, step.value);
	}
	return true;
}

/**
 * The last check before an edit reaches the shared document. The server
 * closes the socket of a client that sends anything outside the contract,
 * and a command, `setMark`, `updateAttributes` or `insertContent` can write
 * an attribute it refuses that no parse rule saw: such a transaction is
 * dropped. What the binding brings in from the shared document is the
 * server's already, and is not checked again.
 */
const ContractGuard = Extension.create({
	name: 'contractGuard',
	addProseMirrorPlugins() {
		return [
			new Plugin({
				key: new PluginKey('contractGuard'),
				filterTransaction: (tr) =>
					tr.getMeta(ySyncPluginKey) !== undefined ||
					tr.steps.every((step, i) => stepFits(step, tr.docs[i])),
			}),
		];
	},
});

function readAttr(el: HTMLElement, spec: AttrSpec): unknown {
	let value = spec.html ? typed(spec, el.getAttribute(spec.html)) : null;
	if (value === null && spec.fromChildClass) {
		const prefix = spec.fromChildClass;
		const child = el.firstElementChild;
		const cls = child ? [...child.classList].find((c) => c.startsWith(prefix)) : undefined;
		if (cls) value = cls.slice(prefix.length);
	}
	return value;
}

function attributesFor(attrs: Record<string, AttrSpec> = {}): Attributes {
	const out: Attributes = {};
	for (const [key, spec] of Object.entries(attrs)) {
		out[key] = {
			default: defaultOf(spec),
			// Null when the element lacks the attribute. Tiptap drops a null
			// parse, so the rule's `set` (h2 → level 2) or the default applies;
			// any other value would override the rule and h2 would parse as h1.
			parseHTML: (el) => readAttr(el, spec),
			// renderHTML below writes attributes itself, in contract order.
			rendered: false,
		};
	}
	return out;
}

function selector(rule: HtmlRule): string {
	const matches = Object.entries(rule.match ?? {})
		.map(([k, v]) => `[${k}="${v}"]`)
		.join('');
	return rule.tag + matches;
}

function parseRule(rule: HtmlRule): TagParseRule {
	return {
		tag: selector(rule),
		// A rule with attribute matches wins over a bare tag, as in the server's ingest.
		priority: 50 + Object.keys(rule.match ?? {}).length * 10,
		...(rule.tag === 'pre' ? { preserveWhitespace: 'full' as const } : {}),
	};
}

/** Attributes the element must yield for the rule to apply: required ones its `set` does not supply. */
function requiredFor(rule: HtmlRule, attrs: Record<string, AttrSpec> = {}): AttrSpec[] {
	return Object.entries(attrs)
		.filter(([key, spec]) => spec.required && !(key in (rule.set ?? {})))
		.map(([, spec]) => spec);
}

/**
 * A node's rules. An element missing a required attribute, or carrying one
 * the server's ingest would refuse (an image whose `src` is `javascript:`, or
 * only a lazy loader's `data-src`), is no node at all: ProseMirror then reads
 * through it, so a failed mention keeps its text. A node built with the
 * attribute null would be written to the shared document, which the server
 * refuses.
 */
function nodeParseRules(spec: NodeSpec): TagParseRule[] {
	return (spec.html ?? []).map((rule) => {
		const required = requiredFor(rule, spec.attrs);
		return {
			...parseRule(rule),
			getAttrs: (el: HTMLElement) =>
				required.some((a) => readAttr(el, a) === null) ? false : { ...(rule.set ?? {}) },
		};
	});
}

/**
 * What a paste from another editor means, on top of the contract's tags.
 * Google Docs wraps every copy in `<b style="font-weight:normal">`, and it and
 * Word say bold, italic, underline and strike with inline styles rather than
 * tags. Read as the stock Tiptap extensions read them. Only pasted HTML takes
 * this path: the server's ingest refuses `style`, and canonical HTML has none.
 */
const PASTE: Record<string, { cancels?: Record<string, (el: HTMLElement) => boolean>; styles?: ParseRule[] }> = {
	bold: {
		cancels: { b: (el) => /^(normal|lighter|[1-4]\d{2})$/.test(el.style.fontWeight) },
		styles: [
			{ style: 'font-weight=400', clearMark: (m) => m.type.name === 'bold' },
			{ style: 'font-weight', getAttrs: (v) => /^(bold(er)?|[5-9]\d{2,})$/.test(v) && null },
		],
	},
	italic: {
		cancels: { i: (el) => el.style.fontStyle === 'normal' },
		styles: [
			{ style: 'font-style=normal', clearMark: (m) => m.type.name === 'italic' },
			{ style: 'font-style=italic' },
		],
	},
	underline: {
		styles: [
			{ style: 'text-decoration', consuming: false, getAttrs: (v) => (v.includes('underline') ? {} : false) },
		],
	},
	strike: {
		styles: [
			{ style: 'text-decoration', consuming: false, getAttrs: (v) => (v.includes('line-through') ? {} : false) },
		],
	},
};

function renderAttrs(
	specs: Record<string, AttrSpec> = {},
	values: Record<string, unknown>,
): Record<string, string> {
	const out: Record<string, string> = {};
	for (const [key, spec] of Object.entries(specs)) {
		if (!spec.html) continue;
		const value = values[key];
		if (value === null || value === undefined || value === defaultOf(spec)) continue;
		out[spec.html] = String(value);
	}
	return out;
}

/**
 * The rule a node renders with: the first writable rule whose `set` matches
 * its attributes. A value of the wrong type (level `"2"` where the contract
 * says int) matches none and throws, which is why the server must write
 * typed Yjs attributes.
 */
function renderRule(name: string, spec: NodeSpec, attrs: Record<string, unknown>): HtmlRule {
	const rule = (spec.html ?? []).find(
		(r) => !r.parseOnly && Object.entries(r.set ?? {}).every(([k, v]) => attrs[k] === v),
	);
	if (!rule) throw new Error(`contract: no render rule for ${name} ${JSON.stringify(attrs)}`);
	return rule;
}

function nodeConfig(name: string, spec: NodeSpec): Partial<NodeConfig> {
	const config: Partial<NodeConfig> = { name };
	if (spec.group !== undefined) config.group = spec.group;
	if (spec.content !== undefined) config.content = spec.content;
	if (spec.marks !== undefined) config.marks = spec.marks;
	if (spec.code !== undefined) config.code = spec.code;
	if (spec.atom !== undefined) config.atom = spec.atom;
	if (spec.inline !== undefined) config.inline = spec.inline;
	if (!spec.html?.length) return config;

	const idAttr = contract.id.attr;
	config.addAttributes = () => ({
		...attributesFor(spec.attrs),
		...(spec.id
			? {
					[idAttr]: {
						default: null,
						parseHTML: (el: HTMLElement) => el.getAttribute(contract.id.html),
						rendered: false,
						// A split block is a new block; the editor gives it a new id.
						keepOnSplit: false,
					},
				}
			: {}),
	});
	config.parseHTML = () => nodeParseRules(spec);
	config.renderHTML = ({ node }): DOMOutputSpec => {
		const rule = renderRule(name, spec, node.attrs);
		const attrs: Record<string, string> = { ...(rule.match ?? {}) };
		if (spec.id && node.attrs[idAttr]) attrs[contract.id.html] = String(node.attrs[idAttr]);
		Object.assign(attrs, renderAttrs(spec.attrs, node.attrs));
		if (spec.content === undefined) return [rule.tag, attrs];
		return rule.wrap ? [rule.tag, attrs, [rule.wrap, 0]] : [rule.tag, attrs, 0];
	};
	return config;
}

function markConfig(name: string, spec: MarkSpec): Partial<MarkConfig> {
	const config: Partial<MarkConfig> = { name };
	if (spec.excludes !== undefined) config.excludes = spec.excludes;
	const paste = PASTE[name] ?? {};
	config.addAttributes = () => attributesFor(spec.attrs);
	config.parseHTML = () => [
		...(spec.html ?? []).map((rule) => {
			const required = requiredFor(rule, spec.attrs);
			const cancels = paste.cancels?.[rule.tag];
			return {
				...parseRule(rule),
				// A required attribute that is missing or refused means no mark;
				// so does a tag its own style sets back to plain.
				getAttrs: (el: HTMLElement) =>
					cancels?.(el) || required.some((a) => readAttr(el, a) === null) ? false : {},
			};
		}),
		...(paste.styles ?? []),
	];
	config.renderHTML = ({ mark }): DOMOutputSpec => {
		const rule = (spec.html ?? []).find((r) => !r.parseOnly);
		if (!rule) throw new Error(`contract: mark ${name} has no writable html rule`);
		return [rule.tag, renderAttrs(spec.attrs, mark.attrs), 0];
	};
	return config;
}

/**
 * Per-type behaviour the editor adds on top of the contract: node views,
 * `selectable`, `draggable`, `defining`. Supplied by the editor, so this
 * module stays free of Svelte.
 */
export type Extras = {
	nodes?: Record<string, Partial<NodeConfig>>;
	marks?: Record<string, Partial<MarkConfig>>;
};

export function contractExtensions(extras: Extras = {}): Extensions {
	const extensions: Extensions = [ContractGuard];
	for (const [name, spec] of Object.entries(contract.nodes)) {
		const config = { ...nodeConfig(name, spec), ...(extras.nodes?.[name] ?? {}) };
		const base = nodeBase(name);
		extensions.push(base ? base.extend(config) : Node.create(config));
	}
	for (const [name, spec] of Object.entries(contract.marks)) {
		const config = { ...markConfig(name, spec), ...(extras.marks?.[name] ?? {}) };
		const base = markBase(name);
		extensions.push(base ? base.extend(config) : Mark.create(config));
	}
	return extensions;
}

/** The ProseMirror schema alone, for parsing and serializing without an editor. */
export function contractSchema(): Schema {
	return getSchema(contractExtensions());
}

/** Node types that carry a block id. */
export const idTypes = Object.entries(contract.nodes)
	.filter(([, spec]) => spec.id)
	.map(([name]) => name);

const ID_ALPHABET = '0123456789abcdefghijklmnopqrstuvwxyz';

/** A block id: the same alphabet and width as the server's. */
export function newId(): string {
	let id = '';
	for (let i = 0; i < 8; i++) id += ID_ALPHABET[Math.floor(Math.random() * ID_ALPHABET.length)];
	return id;
}
