/**
 * composerReserve — the transcript's room for a composer that grows.
 *
 * The composer is absolutely positioned OVER the scroller, so the transcript
 * has to reserve its height itself. That reserve was a fixed 10rem, sized for a
 * one-line pill; a composer grown to its 200px cap (plus attachment previews)
 * overhung it and painted over the tail of the last reply. This writes the live
 * height into a CSS variable the transcript pads from, and re-pins the scroll
 * when the reader was already at the end.
 *
 * Returns a teardown, so the caller is one line of `$effect`.
 */
export function observeComposerReserve(
	composer: HTMLElement,
	scroller: HTMLElement,
): () => void {
	// The composer overlays the scroller, so when a draft grows the composer's
	// top edge climbs into the transcript. The transcript gets the same
	// height back as padding (the CSS var), and the view scrolls by the same
	// amount, so the line that sat just above the composer's old edge sits
	// just above its new one — at any scroll position, not only pinned to
	// the end. A reader who was at the end stays at the end; that case is
	// kept explicit because at send time the new message lands while the
	// composer is still collapsing, and "keep my offset" would leave them
	// one message short of it (VIR-332).
	let lastHeight = composer.offsetHeight;
	scroller.style.setProperty("--composer-height", `${lastHeight}px`);
	const observer = new ResizeObserver(() => {
		const height = composer.offsetHeight;
		const delta = height - lastHeight;
		if (delta === 0) return;
		lastHeight = height;
		// Both reads come BEFORE the padding moves. Shrinking the padding
		// shrinks scrollHeight, and the browser clamps scrollTop to the new
		// end on its own — adding the delta after that clamp would move the
		// reader twice.
		const wasAtBottom =
			scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 8;
		const kept = scroller.scrollTop + delta;
		scroller.style.setProperty("--composer-height", `${height}px`);
		scroller.scrollTop = wasAtBottom ? scroller.scrollHeight : kept;
	});
	observer.observe(composer);
	return () => observer.disconnect();
}
