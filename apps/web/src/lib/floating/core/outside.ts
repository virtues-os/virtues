/**
 * Whether a press lands outside a floating element, for `useClickOutside`.
 *
 * A modal dialog that does not hold the floating element is one opened over
 * it, often by it: a popover's "Restore this version?" confirm. A press in
 * that dialog answers the dialog, and dismissing the popover under it would
 * throw away what the popover shows next (the restore's outcome). A popover
 * inside a modal still closes on a press elsewhere in that modal.
 */

const MODAL = '[aria-modal="true"], .modal-backdrop';

export function pressedOutside(target: EventTarget | null, elements: (HTMLElement | null)[]): boolean {
	const inside = elements.filter((el): el is HTMLElement => el !== null);
	if (!(target instanceof Node)) return true;
	if (inside.some((el) => el.contains(target))) return false;
	const element = target instanceof Element ? target : target.parentElement;
	const modal = element?.closest(MODAL);
	return !modal || inside.some((el) => modal.contains(el));
}
