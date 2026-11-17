/**
 * Light up the thing a person came to find, once ($lib/styles/lit.css): a
 * message you jumped to, a cited passage, the setting a search opened. Every
 * "here it is" in the app is this one wash, so a found thing always looks
 * found.
 *
 * TODO(2026-11-17): migrate DayPage.svelte's cited sentence (the `.flash`
 * class and DayArticleBody's `sentence-flash`), which holds another change
 * in progress.
 */
import '$lib/styles/lit.css';

/** The class that carries the wash, for code that sets classes itself (a ProseMirror decoration). */
export const LIT_CLASS = 'lit';

/** Plays the wash on `el`, from the start if it is already playing. */
export function lightUp(el: Element): void {
	el.classList.remove(LIT_CLASS);
	// Read layout so the class coming back restarts the animation.
	void (el as HTMLElement).offsetWidth;
	el.classList.add(LIT_CLASS);
	const done = () => {
		el.classList.remove(LIT_CLASS);
		el.removeEventListener('animationend', onEnd);
	};
	// animationend bubbles: a child's own animation ending is not the wash's.
	const onEnd = (e: Event) => e.target === el && done();
	el.addEventListener('animationend', onEnd);
	// An element hidden mid-wash never ends its animation.
	setTimeout(done, 2000);
}
