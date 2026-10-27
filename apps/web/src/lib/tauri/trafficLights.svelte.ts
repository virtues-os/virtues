/**
 * The Mac app's traffic lights, as the window header needs them.
 *
 * The shell puts the lights on the header row's centre line and tells the
 * page where the last one ends (src-tauri traffic_lights.rs). In full screen
 * macOS hides them, and the header takes their space back.
 */

import { isMacOS } from '$lib/utils/platform';

/**
 * Where the zoom button ends with the macOS 15 SDK's buttons, which release
 * builds link. Used when the shell does not say.
 */
const DEFAULT_END_PX = 67;

type Shell = { __VIRTUES_TRAFFIC_LIGHTS_END__?: number };

function readEnd(): number {
	if (typeof window === 'undefined') return DEFAULT_END_PX;
	const end = (window as Shell).__VIRTUES_TRAFFIC_LIGHTS_END__;
	return typeof end === 'number' && Number.isFinite(end) ? end : DEFAULT_END_PX;
}

let shown = $state(isMacOS);

async function watchFullScreen() {
	try {
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		const win = getCurrentWindow();
		const read = async () => {
			shown = !(await win.isFullscreen());
		};
		await win.onResized(read);
		await read();
	} catch {
		// An app build without the window API: the lights are taken to be shown.
	}
}

if (isMacOS) void watchFullScreen();

export const trafficLights = {
	/** The zoom button's right edge, in px from the window's left edge. */
	end: readEnd(),
	/** True while the lights are in the header row: the Mac app, out of full screen. */
	get shown() {
		return shown;
	},
};
