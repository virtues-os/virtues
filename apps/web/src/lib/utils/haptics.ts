/**
 * A light haptic tap, for the moments the phone app should feel physical: a
 * sheet or menu opening, a row picked, a thread folding open.
 *
 * It goes through the native shell's haptics plugin, so it only does
 * anything in the iPhone app. Safari has no vibration API, and a desktop has
 * no motor, so everywhere else it is a no-op, and it never throws.
 */
import { isIOS, isTauri } from '$lib/utils/platform';

export function tapHaptic(): void {
	if (!isTauri || !isIOS) return;
	void import('@tauri-apps/api/core')
		.then(({ invoke }) => invoke('plugin:haptics|impact_feedback', { style: 'light' }))
		.catch(() => {});
}
