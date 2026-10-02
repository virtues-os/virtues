/**
 * holdReadingPosition — keep the line being read where it is while the turn
 * that just ended settles.
 *
 * Chromium anchors scroll on its own (`overflow-anchor`): when content above
 * the reader changes height, it moves scrollTop to match. WebKit does not,
 * and WebKit is the phone and the Mac app. The end of a turn is where heights
 * change — the thinking label becomes "Thought for 34s", citations resolve,
 * a finished code block highlights — so a reader scrolled up into the reply
 * watched the text slide out from under them the moment it finished.
 *
 * This is that anchoring by hand, for a short window: note the element at
 * the reading line, and on every frame put it back where it was. Any input
 * from the person ends the hold at once, so it never fights a scroll.
 *
 * Returns a release, so a second turn ending can supersede the first.
 */

/** Long enough for the end-of-turn work that lands a few frames late. */
const HOLD_MS = 700;

/** The blocks worth anchoring to; a lone word span is too easily replaced. */
const BLOCK = "p, li, h1, h2, h3, h4, h5, h6, pre, blockquote, table, .message-wrapper";

const INPUT_EVENTS = ["wheel", "touchstart", "pointerdown", "keydown"] as const;

export function holdReadingPosition(scroller: HTMLElement): () => void {
	const box = scroller.getBoundingClientRect();
	// A little under the top edge, so a header overlapping the scroller is
	// not what gets hit.
	const hit = document.elementFromPoint(box.left + box.width / 2, box.top + 48);
	if (!hit || !scroller.contains(hit)) return () => {};
	const anchor = hit.closest(BLOCK) ?? hit;
	if (!scroller.contains(anchor)) return () => {};

	const offset = anchor.getBoundingClientRect().top - box.top;
	const started = performance.now();
	let frame = 0;

	const release = () => {
		cancelAnimationFrame(frame);
		for (const type of INPUT_EVENTS) scroller.removeEventListener(type, release);
	};

	const tick = () => {
		if (!anchor.isConnected || performance.now() - started > HOLD_MS) {
			release();
			return;
		}
		const drift = anchor.getBoundingClientRect().top - scroller.getBoundingClientRect().top - offset;
		if (Math.abs(drift) >= 1) scroller.scrollTop += drift;
		frame = requestAnimationFrame(tick);
	};

	for (const type of INPUT_EVENTS) scroller.addEventListener(type, release, { passive: true });
	frame = requestAnimationFrame(tick);
	return release;
}
