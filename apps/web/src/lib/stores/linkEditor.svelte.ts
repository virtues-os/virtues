/**
 * Link Editor Store (Svelte 5 Runes)
 *
 * The replacement for "drop the caret on the line and read the raw markdown".
 *
 * That used to be how you fixed a typo in a URL: click the link, the pill
 * reverted to `[label](url)`, you edited the text. Links no longer revert —
 * they render as links whether or not the caret is near them — so the label and
 * the href need somewhere to be edited that is not the document surface.
 *
 * Follows the IconPicker pattern: a global store, one panel mounted in the app
 * layout, anchored to whatever was clicked rather than centered over the
 * document it is asking about.
 */

/** A point to hang the panel off — usually the link that opened it. */
export interface LinkEditorAnchor {
	x: number;
	y: number;
	width: number;
	height: number;
}

export interface LinkEditorValue {
	label: string;
	href: string;
}

class LinkEditorStore {
	open = $state(false);
	label = $state('');
	href = $state('');
	anchor = $state<LinkEditorAnchor | null>(null);
	private _onSave: ((value: LinkEditorValue) => void) | null = null;

	show(
		value: LinkEditorValue,
		onSave: (value: LinkEditorValue) => void,
		anchor?: LinkEditorAnchor,
	) {
		this.label = value.label;
		this.href = value.href;
		this.anchor = anchor ?? null;
		this._onSave = onSave;
		this.open = true;
	}

	save() {
		// An empty href would render as a link to nowhere; an empty label would
		// render as nothing at all. Either one is a worse document than the one
		// the user started with, so refuse rather than silently write it.
		const label = this.label.trim();
		const href = this.href.trim();
		if (!label || !href) return;

		this._onSave?.({ label, href });
		this.hide();
	}

	hide() {
		this.open = false;
		this.anchor = null;
		this._onSave = null;
	}
}

export const linkEditor = new LinkEditorStore();
