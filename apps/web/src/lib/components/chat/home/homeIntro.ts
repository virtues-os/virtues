/**
 * Home's intro plays once per launch of the app: the first time a chat view
 * shows home. Every later home appears the ordinary way, because the same
 * movement on every new chat would grow tiresome.
 */
let played = false;

/** True the first time it is asked after the app opens, false after. */
export function takeIntro(): boolean {
	if (played) return false;
	played = true;
	return true;
}
