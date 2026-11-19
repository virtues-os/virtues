/**
 * A wait that only shows itself once it has gone on a while: true when
 * `waiting()` has been true for `ms`. A page that loads quickly shows no
 * spinner or "Loading" line at all and rises straight into place; a slow
 * one says it is working.
 *
 * Call it while a component initialises, like any rune.
 */
export const LATE_MS = 400;

export function lateFlag(waiting: () => boolean, ms = LATE_MS): { readonly current: boolean } {
	let late = $state(false);
	$effect(() => {
		late = false;
		if (!waiting()) return;
		const timer = setTimeout(() => (late = true), ms);
		return () => clearTimeout(timer);
	});
	return {
		get current() {
			return late;
		},
	};
}
