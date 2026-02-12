/**
 * Shared Icon Picker Store (Svelte 5 Runes)
 *
 * Allows any component to trigger the global IconPicker modal.
 */

class IconPickerStore {
	open = $state(false);
	currentValue = $state<string | null>(null);
	private _onSelect: ((icon: string | null) => void) | null = null;

	show(currentValue: string | null, onSelect: (icon: string | null) => void) {
		this.currentValue = currentValue;
		this._onSelect = onSelect;
		this.open = true;
	}

	select(icon: string | null) {
		this._onSelect?.(icon);
		this.hide();
	}

	hide() {
		this.open = false;
		this._onSelect = null;
	}
}

export const iconPickerStore = new IconPickerStore();
