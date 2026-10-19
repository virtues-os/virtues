/**
 * Whether a step is the thing in front of the person.
 *
 * On the stage it always is. In the app (SetupView) it may not be: the app
 * keeps a tab that isn't showing mounted, and a split puts another pane
 * beside it. A step there must not take the keyboard from the other pane,
 * answer keys pressed in it (⌘Z in the chat's editor), or keep polling while
 * nobody can see it. SetupView provides the answer; the stage provides none,
 * which reads as always showing.
 */
import { getContext, setContext } from 'svelte';

const KEY = Symbol('setup-showing');

export function provideShowing(showing: () => boolean): void {
	setContext(KEY, showing);
}

/** Called during a step's setup, like any context read. */
export function useShowing(): () => boolean {
	return getContext<(() => boolean) | undefined>(KEY) ?? (() => true);
}

/** The pane an element is in; null on the stage, which has none. */
function paneOf(el: Element | null): Element | null {
	return el?.closest('.tab-content') ?? null;
}

/** A step may move focus to `el` only while it is showing and focus isn't
 *  in another pane. */
export function mayTakeFocus(showing: () => boolean, el: Element | null | undefined): boolean {
	if (!el || !showing()) return false;
	const at = document.activeElement;
	if (!at || at === document.body) return true;
	return paneOf(at) === paneOf(el);
}

/** A window-level event is the step's only while it is showing and the event
 *  didn't happen in another pane. */
export function isOurs(showing: () => boolean, e: Event, root: Element | null | undefined): boolean {
	if (!showing()) return false;
	const t = e.target;
	if (!(t instanceof Element) || t === document.body || t === document.documentElement) return true;
	return paneOf(t) === paneOf(root ?? null);
}
