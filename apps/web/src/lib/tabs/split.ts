/**
 * Splitting: the tab you move becomes the new pane. The tab travels from
 * its place in the first strip to the new pane's strip on a soft spring,
 * and the pane's card opens out from just under where the tab was, growing
 * across and down to its full size while the first page narrows. The page
 * fades in as the card lands, and the card hands over to it.
 *
 * Both cards move on the same spring for the length of the split
 * (`beginTabToPane`, which has to run before the split is drawn, so the
 * cards' width transitions start on it). The tab and the card that travel
 * are stand-ins drawn over everything for the length of the spring: the new
 * strip is still growing from nothing and would clip the real tab. SplitContainer's own motion (the right card growing from the
 * window's edge, the view clipped in behind it) still runs underneath, held
 * invisible until the stand-in lands. Reduced motion skips all of it and
 * leaves SplitContainer's motion, which app.css stills.
 *
 * Home and a new chat stand on glass, which an ancestor's fade would cut
 * off from the painting, so a new chat moved across opens the old way.
 */
import { reducedMotion } from '$lib/stores/motion.svelte';
import { springEasing } from '$lib/utils/springEasing';

const SPRING = springEasing(260, 26);

interface Rect {
	left: number;
	top: number;
	width: number;
	height: number;
}

/**
 * Put both cards on the spring for the split about to be drawn. Returns
 * false under reduced motion, when nothing of this plays.
 */
export function beginTabToPane(container: HTMLElement): boolean {
	if (reducedMotion()) return false;
	container.style.setProperty('--split-ms', `${SPRING.duration}ms`);
	container.style.setProperty('--split-ease', SPRING.easing);
	container.classList.add('tab-to-pane');
	setTimeout(() => container.classList.remove('tab-to-pane'), SPRING.duration + 100);
	return true;
}

/** Where a tab sits on screen, read before the split changes the strips. */
export function tabRect(tabId: string): DOMRect | null {
	const chip = document.querySelector(`.tab[data-tab-id="${CSS.escape(tabId)}"]:not(.closing)`);
	return chip?.getBoundingClientRect() ?? null;
}

/**
 * Play the tab becoming the pane. `from` is the tab's rect before the split;
 * `pane` is the new pane's final box inside `container`. Call it once the
 * split has been drawn.
 */
export function tabBecomesPane(container: HTMLElement, tabId: string, from: DOMRect, pane: Rect): void {
	if (reducedMotion()) return;
	const slot = container.querySelector<HTMLElement>(`.tab-slot[data-tab-id="${CSS.escape(tabId)}"]`);
	const shell = container.querySelector<HTMLElement>('.pane-shell.right-pane');
	if (!slot || !shell) return;
	if (slot.querySelector('.page-container.home')) return;
	const box = container.getBoundingClientRect();

	// The tab: from its old place to its new one. The old tab is folding
	// out of the first strip; it goes at once so there is only one.
	const id = CSS.escape(tabId);
	document.querySelector<HTMLElement>(`.tab.closing[data-tab-id="${id}"]`)?.style.setProperty('visibility', 'hidden');
	const chip = document.querySelector<HTMLElement>(`.tab[data-tab-id="${id}"]:not(.closing)`);
	// Where it lands: the new strip is still growing, so its tab is not in
	// place yet. A strip's first tab starts at its card's left edge.
	if (chip) travel(chip, from, { left: box.left + pane.left, top: from.top, width: from.width, height: from.height });

	// The card: out from under the tab to the pane's box.
	const card = document.createElement('div');
	card.className = 'pane-stand-in';
	container.append(card);
	const start = {
		left: `${from.left - box.left}px`,
		top: '0px',
		width: `${from.width}px`,
		height: `${from.height}px`,
	};
	const end = {
		left: `${pane.left}px`,
		top: `${pane.top}px`,
		width: `${pane.width}px`,
		height: `${pane.height}px`,
	};
	const grow = card.animate([start, end], { duration: SPRING.duration, easing: SPRING.easing, fill: 'forwards' });

	// The real pane and its page wait under the stand-in, then fade in as
	// it lands; the stand-in fades as they arrive.
	const held = [{ opacity: 0 }, { opacity: 0, offset: 0.6 }, { opacity: 1 }];
	const timing = { duration: SPRING.duration + 80, easing: 'linear' };
	shell.animate(held, timing);
	slot.animate(held, timing);
	void grow.finished.then(() => {
		const out = card.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 120, easing: 'linear', fill: 'forwards' });
		void out.finished.then(() => card.remove());
	}, () => card.remove());
}

/** The tab's stand-in, drawn over everything, from `from` to `to`. */
function travel(chip: HTMLElement, from: DOMRect, to: Rect) {
	const look = getComputedStyle(chip);
	const stand = chip.cloneNode(true) as HTMLElement;
	stand.removeAttribute('data-tab-id');
	stand.setAttribute('aria-hidden', 'true');
	Object.assign(stand.style, {
		position: 'fixed',
		margin: '0',
		zIndex: 'var(--z-overlay)',
		pointerEvents: 'none',
		fontFamily: look.fontFamily,
		fontSize: look.fontSize,
		color: look.color,
		boxSizing: 'border-box',
	});
	document.body.append(stand);
	const timing = { duration: SPRING.duration, easing: SPRING.easing, fill: 'forwards' as const };
	const go = stand.animate(
		[
			{ left: `${from.left}px`, top: `${from.top}px`, width: `${from.width}px`, height: `${from.height}px` },
			{ left: `${to.left}px`, top: `${to.top}px`, width: `${to.width}px`, height: `${to.height}px` },
		],
		timing,
	);
	// The real tab waits under its stand-in until it lands.
	chip.animate([{ opacity: 0 }, { opacity: 0 }], { duration: SPRING.duration });
	void go.finished.then(() => stand.remove(), () => stand.remove());
}
