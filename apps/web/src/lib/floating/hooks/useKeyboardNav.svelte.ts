/**
 * useKeyboardNav Hook
 *
 * Provides keyboard navigation for lists/menus.
 * Supports arrow keys, Enter/Space to select, Escape to close, Home/End to jump.
 */

export interface KeyboardNavOptions<T> {
	items: () => T[];
	onSelect: (item: T, index: number) => void;
	onEscape: () => void;
	enabled?: () => boolean;
	loop?: boolean;
}

export function useKeyboardNav<T>(options: KeyboardNavOptions<T>) {
	const { items, onSelect, onEscape, enabled = () => true, loop = true } = options;

	let selectedIndex = $state(-1);

	function moveUp() {
		const list = items();
		if (list.length === 0) return;

		if (selectedIndex <= 0) {
			selectedIndex = loop ? list.length - 1 : 0;
		} else {
			selectedIndex--;
		}
	}

	function moveDown() {
		const list = items();
		if (list.length === 0) return;

		if (selectedIndex >= list.length - 1) {
			selectedIndex = loop ? 0 : list.length - 1;
		} else {
			selectedIndex++;
		}
	}

	function select() {
		const list = items();
		if (selectedIndex >= 0 && selectedIndex < list.length) {
			onSelect(list[selectedIndex], selectedIndex);
		}
	}

	function reset() {
		selectedIndex = -1;
	}

	function setIndex(index: number) {
		selectedIndex = index;
	}

	$effect(() => {
		if (!enabled()) return;

		function handleKeydown(event: KeyboardEvent) {
			switch (event.key) {
				case 'ArrowDown':
					event.preventDefault();
					moveDown();
					break;
				case 'ArrowUp':
					event.preventDefault();
					moveUp();
					break;
				case 'Enter':
				case ' ':
					event.preventDefault();
					select();
					break;
				case 'Escape':
					event.preventDefault();
					onEscape();
					break;
				case 'Home':
					event.preventDefault();
					selectedIndex = 0;
					break;
				case 'End':
					event.preventDefault();
					selectedIndex = items().length - 1;
					break;
			}
		}

		document.addEventListener('keydown', handleKeydown);
		return () => document.removeEventListener('keydown', handleKeydown);
	});

	return {
		get selectedIndex() {
			return selectedIndex;
		},
		set selectedIndex(i: number) {
			selectedIndex = i;
		},
		moveUp,
		moveDown,
		select,
		reset,
		setIndex
	};
}
