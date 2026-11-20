/**
 * Follow the day, the part that runs after the first paint (theme.ts loads
 * it when the choice is `day`): repaint at the next sunrise or sunset, and
 * when the window comes back, since a sleeping laptop misses its timer. Sun
 * times come from the server's weather forecast for today, which is made for
 * its last known location; without one the day runs from 07:00 to 19:00.
 */
import { request } from '$lib/api/client';
import { SUN_STORAGE_KEY, minutesOf, sunTimes } from './theme';

/** Start following the day. Returns the function that stops it. */
export function followDay(repaint: () => void): () => void {
	let timer: ReturnType<typeof setTimeout> | null = null;
	let stopped = false;

	const schedule = () => {
		if (timer) clearTimeout(timer);
		if (stopped) return;
		const [rise, set] = sunTimes();
		const now = new Date();
		const m = minutesOf(now);
		const next = m < rise ? rise : m < set ? set : rise + 24 * 60;
		const at = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 0, next, 5);
		timer = setTimeout(() => {
			repaint();
			void refreshSun().then(() => !stopped && repaint());
			schedule();
		}, at.getTime() - now.getTime());
	};
	const onVisible = () => {
		if (document.visibilityState !== 'visible') return;
		repaint();
		schedule();
	};

	document.addEventListener('visibilitychange', onVisible);
	schedule();
	void refreshSun().then(() => {
		if (stopped) return;
		repaint();
		schedule();
	});

	return () => {
		stopped = true;
		if (timer) clearTimeout(timer);
		document.removeEventListener('visibilitychange', onVisible);
	};
}

/** Today's sunrise and sunset from the server's forecast, cached for the pre-paint. */
async function refreshSun(): Promise<void> {
	try {
		const weather = await request<{ sunrise?: string | null; sunset?: string | null } | null>('/weather/current');
		if (!weather?.sunrise || !weather.sunset) return;
		const rise = minutesOf(new Date(weather.sunrise));
		const set = minutesOf(new Date(weather.sunset));
		if (rise < set) localStorage.setItem(SUN_STORAGE_KEY, `${rise} ${set}`);
	} catch {
		// No forecast or no server: keep the cached or default hours.
	}
}
