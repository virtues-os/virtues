/**
 * The owner's units (Settings · Profile · Units), kept in step with
 * `utils/units`' module value, so plain functions and pages read the same
 * thing. The app's layout load seeds it from the profile it already fetches,
 * before the shell draws anything that formats a distance; US units stand in
 * when that load could not read the profile. `set` is what the Units row
 * calls after it saves.
 */
import type { Profile } from '$lib/api/client';
import { setPreferredUnits, type Units } from '$lib/utils/units';

class UnitsStore {
	current = $state<Units>('us');

	/** Take the profile the app's layout load fetched, rather than asking again. */
	seed(profile: Pick<Profile, 'units'>) {
		this.set(profile.units === 'metric' ? 'metric' : 'us');
	}

	set(units: Units) {
		this.current = units;
		setPreferredUnits(units);
	}
}

export const units = new UnitsStore();
