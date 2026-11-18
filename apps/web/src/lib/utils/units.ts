/**
 * Distances and temperatures in the units the owner reads in (Settings ·
 * Profile · Units, `app_user_profile.units`). The numbers arrive metric
 * (meters, °C); these turn them into words.
 *
 * The preference is a module value so plain functions (the timeline's titles)
 * can read it without a component. `stores/units.svelte` loads it from the
 * profile and keeps it in step, reactively, for the pages that draw it.
 */

export type Units = 'us' | 'metric';

let preferred: Units = 'us';

export function preferredUnits(): Units {
	return preferred;
}

export function setPreferredUnits(units: Units): void {
	preferred = units;
}

const METERS_PER_MILE = 1609.344;
const METERS_PER_FOOT = 0.3048;

/** "12 km", "3.4 km", "800 m"; or "7.5 mi", "12 mi", "500 ft". */
export function formatDistance(meters: number, units: Units = preferred): string {
	if (units === 'metric') {
		if (meters < 1000) return `${Math.round(meters / 10) * 10} m`;
		const km = meters / 1000;
		return `${km < 10 ? km.toFixed(1) : Math.round(km)} km`;
	}
	const miles = meters / METERS_PER_MILE;
	if (miles < 0.1) return `${Math.round(meters / METERS_PER_FOOT / 10) * 10} ft`;
	return `${miles < 10 ? miles.toFixed(1) : Math.round(miles)} mi`;
}

/** A temperature as a whole number of degrees: "71°" or "22°", or with its
 *  scale ("71 °F") where nothing around it says which. */
export function formatTemperature(celsius: number, units: Units = preferred, opts: { scale?: boolean } = {}): string {
	const degrees = Math.round(units === 'metric' ? celsius : (celsius * 9) / 5 + 32);
	if (!opts.scale) return `${degrees}°`;
	return `${degrees} °${units === 'metric' ? 'C' : 'F'}`;
}
