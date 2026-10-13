/**
 * A menu that opens while the focus stays where it was: a block page's
 * editor keeps it, so the caret and the keyboard stay, and the arrows and
 * Enter are taken from the window (`keys.ts`). A screen reader follows the
 * focus, so the element holding it is told which list it controls and which
 * row the arrows have reached, as a combobox's input is; the row is read as
 * it is reached.
 *
 * Only some roles can say which row is reached (`aria-activedescendant`): a
 * text box, a combobox, a list. A button, a link or a slider cannot, so a
 * menu opened from one (the table's More, a callout's kind, Shift-F10 on a
 * link or a size handle) would be silent: from those the list takes the
 * focus and says it itself, and gives the focus back as it closes.
 */

export interface PointedAt {
	/** The row the arrows have reached, by id; null for none. */
	highlight(rowId: string | null): void;
	/** The menu closed: the element says again what it said before, and has the focus back if the list took it. */
	release(): void;
}

const SAID = ['aria-controls', 'aria-activedescendant', 'aria-expanded'] as const;

/** The roles that may say which of their rows is reached (ARIA's `aria-activedescendant`). */
const HOLDS_ACTIVE = new Set([
	'application',
	'combobox',
	'grid',
	'group',
	'listbox',
	'menu',
	'menubar',
	'radiogroup',
	'row',
	'searchbox',
	'spinbutton',
	'tablist',
	'textbox',
	'toolbar',
	'tree',
	'treegrid',
]);

/** Whether `el` may say which row is reached: by its role, or as a text field or an editor is. */
export function holdsActiveDescendant(el: Element): boolean {
	const role = el.getAttribute('role')?.trim().split(/\s+/)[0];
	if (role) return HOLDS_ACTIVE.has(role);
	if (el.tagName === 'TEXTAREA' || el.tagName === 'SELECT') return true;
	if (el.tagName === 'INPUT') return ['', 'text', 'search', 'email', 'url', 'tel', 'number'].includes((el as HTMLInputElement).type);
	const editable = el.getAttribute('contenteditable');
	return editable === '' || editable === 'true' || editable === 'plaintext-only';
}

/**
 * Tell `owner`, the element with the focus, that it controls the list
 * `listId`. When it cannot say which row is reached, the list (found by its
 * id when a row is first reached) takes the focus and says it.
 */
export function pointFocusAt(owner: Element, listId: string): PointedAt {
	const before = SAID.map((name) => [name, owner.getAttribute(name)] as const);
	owner.setAttribute('aria-controls', listId);
	// Only a control that opens a popup takes `aria-expanded`; a text box does not.
	const expands = owner.tagName === 'BUTTON' || ['button', 'combobox'].includes(owner.getAttribute('role') ?? '');
	if (expands) owner.setAttribute('aria-expanded', 'true');
	const own = holdsActiveDescendant(owner);
	let list: HTMLElement | null = null;
	return {
		highlight(rowId) {
			let teller: Element = owner;
			if (!own) {
				const el = document.getElementById(listId);
				if (!el) return;
				if (el !== list) {
					list = el;
					if (!el.hasAttribute('tabindex')) el.setAttribute('tabindex', '-1');
					el.focus({ preventScroll: true });
				}
				teller = el;
			}
			if (rowId) teller.setAttribute('aria-activedescendant', rowId);
			else teller.removeAttribute('aria-activedescendant');
		},
		release() {
			for (const [name, value] of before) {
				if (value === null) owner.removeAttribute(name);
				else owner.setAttribute(name, value);
			}
			if (!list) return;
			// Back where it was, unless what the menu ran put it elsewhere.
			const now = document.activeElement;
			const lost = !now || now === document.body || now === document.documentElement || list.contains(now);
			list = null;
			if (lost && owner.isConnected) (owner as HTMLElement).focus({ preventScroll: true });
		},
	};
}

/** The element the focus is in, when it is one that can be told: not the page itself. */
export function focusOwner(): Element | null {
	if (typeof document === 'undefined') return null;
	const el = document.activeElement;
	return el && el !== document.body && el !== document.documentElement ? el : null;
}
