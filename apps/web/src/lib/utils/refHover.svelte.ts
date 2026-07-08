// Hover/focus intent for reference previews. Opens after a short dwell (so
// passing the cursor over a pill doesn't flash a card), closes with a grace
// period (so the pointer can travel from pill → card without it vanishing).
// Shared by every Svelte ref renderer (Ref, LinkChip).

const SHOW_DELAY = 350;
const HIDE_DELAY = 160;

export function createRefHover() {
	let visible = $state(false);
	let anchor = $state<HTMLElement | null>(null);
	let showTimer: ReturnType<typeof setTimeout> | null = null;
	let hideTimer: ReturnType<typeof setTimeout> | null = null;

	function clearShow() {
		if (showTimer) {
			clearTimeout(showTimer);
			showTimer = null;
		}
	}
	function clearHide() {
		if (hideTimer) {
			clearTimeout(hideTimer);
			hideTimer = null;
		}
	}

	return {
		get visible() {
			return visible;
		},
		get anchor() {
			return anchor;
		},
		/** Pointer/focus entered the pill. */
		enter(el: HTMLElement) {
			clearHide();
			anchor = el;
			if (visible) return;
			clearShow();
			showTimer = setTimeout(() => {
				visible = true;
				showTimer = null;
			}, SHOW_DELAY);
		},
		/** Pointer/focus left the pill (or the card). */
		leave() {
			clearShow();
			clearHide();
			hideTimer = setTimeout(() => {
				visible = false;
				hideTimer = null;
			}, HIDE_DELAY);
		},
		/** Pointer entered the card — keep it open. */
		cancelHide() {
			clearHide();
		},
		/** Show immediately (plain click / keyboard), skipping the dwell delay. */
		pin(el: HTMLElement) {
			clearShow();
			clearHide();
			anchor = el;
			visible = true;
		},
	};
}
