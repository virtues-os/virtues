/**
 * The veil: day pages with their names and hard passages hidden, for reading
 * with someone else in the room. A per-device setting, because it answers
 * "who can see this screen", which is a fact about the device.
 *
 * While the veil is on, holding `V` lifts it; letting go puts it back, so it
 * cannot be left lifted by accident. `V` is ignored while you are typing.
 * What gets hidden, and how, is `lib/actions/veil.ts`.
 */

const STORAGE_KEY = "virtues.veil";

function load(): boolean {
	try {
		return localStorage.getItem(STORAGE_KEY) === "on";
	} catch {
		// Storage unavailable (private window, SSR): the veil starts off.
		return false;
	}
}

function typing(target: EventTarget | null): boolean {
	const el = target as HTMLElement | null;
	if (!el) return false;
	return el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName);
}

class Veil {
	on = $state(false);
	/** True while `V` is held with the veil on. */
	lifted = $state(false);
	#bound = false;

	constructor() {
		if (typeof window === "undefined") return;
		this.on = load();
		this.#bind();
	}

	/** Hidden right now: on, and not being lifted. */
	get hiding(): boolean {
		return this.on && !this.lifted;
	}

	toggle() {
		this.on = !this.on;
		this.lifted = false;
		try {
			localStorage.setItem(STORAGE_KEY, this.on ? "on" : "off");
		} catch {
			// Storage unavailable: the setting lasts for this session only.
		}
	}

	#bind() {
		if (this.#bound) return;
		this.#bound = true;
		window.addEventListener("keydown", (e) => {
			if (!this.on || e.repeat || e.metaKey || e.ctrlKey || e.altKey) return;
			if (e.key.toLowerCase() !== "v" || typing(e.target)) return;
			this.lifted = true;
		});
		window.addEventListener("keyup", (e) => {
			if (e.key.toLowerCase() === "v") this.lifted = false;
		});
		// A lifted veil must not survive the window losing focus mid-hold.
		window.addEventListener("blur", () => (this.lifted = false));
	}
}

export const veil = new Veil();
