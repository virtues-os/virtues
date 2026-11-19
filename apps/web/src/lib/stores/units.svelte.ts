/**
 * The owner's units (Settings · Profile · Units), loaded from the profile
 * once and kept in step with `utils/units`' module value, so plain functions
 * and pages read the same thing. `set` is what the Units row calls after it
 * saves.
 */
import { getProfile } from '$lib/api/client';
import { setPreferredUnits, type Units } from '$lib/utils/units';

class UnitsStore {
	current = $state<Units>('us');

	constructor() {
		if (typeof window === 'undefined') return;
		getProfile()
			.then((p) => this.set(p.units === 'metric' ? 'metric' : 'us'))
			.catch(() => {
				// US units stand in until the next load; nothing here saves them.
			});
	}

	set(units: Units) {
		this.current = units;
		setPreferredUnits(units);
	}
}

export const units = new UnitsStore();
