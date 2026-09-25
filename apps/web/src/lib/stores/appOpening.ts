/**
 * The app opening — a one-shot handoff from Setup to the shell.
 *
 * Setup and the app live in different route groups with different layouts,
 * so nothing on the page survives the navigation between them. Setup raises
 * this flag just before it navigates; the app shell reads it ONCE on its
 * first render and plays its opening (the ∴ beats, the rail cascades out of
 * it, the panel and pane settle, the Setup tile lights if steps remain).
 * Every later mount of the shell — a reload, a return from Settings — finds
 * it lowered and opens plainly.
 *
 * Module state, not storage: a reload in the middle of it should simply not
 * play it, which is exactly what a module flag does.
 */
let pending = false;

export const appOpening = {
	arm(): void {
		pending = true;
	},
	/** True once, for the first shell render after the letter. */
	consume(): boolean {
		const was = pending;
		pending = false;
		return was;
	},
};

/**
 * THE APP OPENING, from whatever full-screen page precedes the shell (Setup's
 * close today). The page is set down and the app opens beneath it: a view
 * transition, so the page holds its last frame until the shell is ready
 * rather than cutting to a blank desk, then the shell plays its own opening,
 * armed here and consumed once by the app layout. Without view transitions
 * the caller's own fade runs first (`fallback`); with reduced motion it is a
 * plain navigation.
 */
export async function openApp(next: string, fallback?: () => Promise<void>): Promise<void> {
	const { goto } = await import('$app/navigation');
	const reduced =
		typeof window !== 'undefined' && !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
	if (reduced) return goto(next);
	appOpening.arm();
	const doc = document as Document & {
		startViewTransition?: (cb: () => Promise<void>) => { finished: Promise<void> };
	};
	if (doc.startViewTransition) {
		const root = document.documentElement;
		root.classList.add('app-opening');
		const t = doc.startViewTransition(async () => {
			await goto(next);
		});
		t.finished.finally(() => root.classList.remove('app-opening'));
		return;
	}
	if (fallback) await fallback();
	await goto(next);
}
