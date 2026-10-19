/**
 * Shared Icon Picker Store (Svelte 5 Runes)
 *
 * Allows any component to trigger the global IconPicker modal.
 *
 * An icon and its color are one decision made in one panel (see IconPicker),
 * so the store carries both. `onColorSelect` is optional: a caller whose
 * entity has nowhere to persist a color omits it and the swatch row doesn't
 * render, rather than offering a control that silently forgets.
 */

/** A point to hang the panel off — usually the click that opened it. */
export interface PickerAnchor {
	x: number;
	y: number;
	width: number;
	height: number;
}

interface ShowOptions {
	/** Current `--cat-*` token key, or null for "no color". */
	color?: string | null;
	/** Persist a color choice. Fires without closing the picker. */
	onColorSelect?: (color: string | null) => void;
	/**
	 * Where to put the panel. Omit and it lands in the middle of the window,
	 * which is what a centered modal did and why this exists: a picker for one
	 * small button 400px away covered the document to ask about an icon.
	 */
	anchor?: PickerAnchor;
}

class IconPickerStore {
	open = $state(false);
	currentValue = $state<string | null>(null);
	currentColor = $state<string | null>(null);
	/** Whether this invocation offers a color at all. */
	colorEnabled = $state(false);
	anchor = $state<PickerAnchor | null>(null);
	private _onSelect: ((icon: string | null) => void) | null = null;
	private _onColorSelect: ((color: string | null) => void) | null = null;

	show(
		currentValue: string | null,
		onSelect: (icon: string | null) => void,
		options: ShowOptions = {},
	) {
		this.currentValue = currentValue;
		this.currentColor = options.color ?? null;
		this.colorEnabled = !!options.onColorSelect;
		this.anchor = options.anchor ?? null;
		this._onSelect = onSelect;
		this._onColorSelect = options.onColorSelect ?? null;
		this.open = true;
	}

	select(icon: string | null) {
		this._onSelect?.(icon);
		this.hide();
	}

	/**
	 * Set the color WITHOUT closing — a color is a preview you hold against the
	 * icon grid, and closing on it would make choosing both require reopening
	 * the panel.
	 */
	selectColor(color: string | null) {
		this.currentColor = color;
		this._onColorSelect?.(color);
	}

	hide() {
		this.open = false;
		this.colorEnabled = false;
		this.anchor = null;
		this._onSelect = null;
		this._onColorSelect = null;
	}
}

export const iconPickerStore = new IconPickerStore();
