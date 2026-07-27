/**
 * The keyboard shortcut registry.
 *
 * One `keydown` listener for the whole app, fed by declarative bindings, so
 * that:
 *
 *  · shortcuts are discoverable — `shortcuts.all` is the cheat sheet, and it
 *    can't drift from what actually fires, because it *is* what fires;
 *  · modifiers match exactly. The hand-rolled chain this replaces tested
 *    `metaKey && key === 's'` without excluding Shift, so ⌘⇧S quietly
 *    collapsed the sidebar on its way to whatever the user meant;
 *  · conflicts are detectable rather than decided by source order — two
 *    bindings on the same chord now warn in dev instead of silently both
 *    firing;
 *  · rebinding later is a data change, not a refactor.
 *
 * Component-local handlers (a modal's Escape, an editor's Tab) deliberately
 * stay where they are. They're scoped to a focused thing and are correct as
 * local listeners; only *global* shortcuts belong here.
 */

import { isAppleKeyboard } from '$lib/utils/platform';

/** Canonical chord: lowercase parts, `mod` for ⌘/Ctrl, `+`-joined. */
export type Chord = string;

export interface Shortcut {
	/** Stable id, also the dedupe key. */
	id: string;
	/** e.g. `mod+k`, `mod+shift+n`. `mod` is ⌘ on Apple, Ctrl elsewhere. */
	keys: Chord;
	/** Shown in the cheat sheet. Imperative: "New chat", not "Creates a chat". */
	label: string;
	/** Cheat-sheet grouping. */
	group?: string;
	run: (event: KeyboardEvent) => void;
	/**
	 * Fire while a text field has focus. Defaults to true for `mod` chords
	 * (⌘K must work mid-sentence) and false otherwise, so bare-key shortcuts
	 * can never eat someone's typing.
	 */
	allowInInput?: boolean;
	/** Skip preventDefault — rare; only when the browser default is wanted too. */
	passive?: boolean;
}

const MODIFIERS = ['mod', 'meta', 'ctrl', 'alt', 'shift'] as const;

/** Normalise so `Mod+Shift+N`, `shift+mod+n` and `mod+shift+n` are one chord. */
function normalize(keys: Chord): Chord {
	const parts = keys
		.toLowerCase()
		.split('+')
		.map((p) => p.trim())
		.filter(Boolean);

	const mods = MODIFIERS.filter((m) => parts.includes(m));
	const rest = parts.filter((p) => !MODIFIERS.includes(p as never)).sort();
	return [...mods, ...rest].join('+');
}

/** The chord an event represents, in the same normal form. */
function chordOf(event: KeyboardEvent): Chord {
	const parts: string[] = [];

	// `mod` is the platform's primary accelerator. Matching it to the physical
	// key (meta on Apple, ctrl elsewhere) means one binding covers both.
	const modPressed = isAppleKeyboard ? event.metaKey : event.ctrlKey;
	if (modPressed) parts.push('mod');
	if (!isAppleKeyboard && event.metaKey) parts.push('meta');
	if (isAppleKeyboard && event.ctrlKey) parts.push('ctrl');
	if (event.altKey) parts.push('alt');
	if (event.shiftKey) parts.push('shift');

	// `event.key` shifts with modifiers ("N" vs "n", "†" for ⌥T on macOS), so
	// the chord is keyed off physical position via `code` where we can get it.
	// Falls back to `key` for synthetic events and non-alphanumerics.
	let base = '';
	if (event.code?.startsWith('Key')) base = event.code.slice(3).toLowerCase();
	else if (event.code?.startsWith('Digit')) base = event.code.slice(5);
	else if (event.key) base = event.key.toLowerCase();

	if (base) parts.push(base);
	return parts.join('+');
}

function isTextEntry(target: EventTarget | null): boolean {
	if (!(target instanceof HTMLElement)) return false;
	if (target.isContentEditable) return true;
	const tag = target.tagName;
	if (tag === 'TEXTAREA' || tag === 'SELECT') return true;
	if (tag !== 'INPUT') return false;
	// Checkboxes and buttons aren't text entry — shortcuts should still fire.
	const type = (target as HTMLInputElement).type;
	return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range'].includes(type);
}

class ShortcutRegistry {
	#shortcuts = $state<Shortcut[]>([]);
	#listening = false;

	/** Everything currently bound — the cheat sheet's data source. */
	get all(): Shortcut[] {
		return this.#shortcuts;
	}

	/**
	 * Bind shortcuts; returns an unregister for onMount teardown. Re-registering
	 * the same id replaces it, so hot reload doesn't stack duplicates.
	 */
	register(...shortcuts: Shortcut[]): () => void {
		const ids = new Set(shortcuts.map((s) => s.id));
		this.#shortcuts = [...this.#shortcuts.filter((s) => !ids.has(s.id)), ...shortcuts];

		if (import.meta.env.DEV) {
			const seen = new Map<Chord, string>();
			for (const s of this.#shortcuts) {
				const chord = normalize(s.keys);
				const prior = seen.get(chord);
				if (prior && prior !== s.id) {
					console.warn(
						`[shortcuts] "${chord}" is bound by both "${prior}" and "${s.id}" — ` +
							`both will fire, in registration order.`,
					);
				}
				seen.set(chord, s.id);
			}
		}

		this.#listen();
		return () => {
			this.#shortcuts = this.#shortcuts.filter((s) => !ids.has(s.id));
		};
	}

	/** Render a chord for display: `mod+shift+n` → `⌘⇧N` / `Ctrl+Shift+N`. */
	format(keys: Chord): string {
		const parts = normalize(keys).split('+');
		const glyphs: Record<string, string> = isAppleKeyboard
			? { mod: '⌘', meta: '⌘', ctrl: '⌃', alt: '⌥', shift: '⇧' }
			: { mod: 'Ctrl', meta: 'Win', ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift' };

		const rendered = parts.map((p) => glyphs[p] ?? p.toUpperCase());
		return isAppleKeyboard ? rendered.join('') : rendered.join('+');
	}

	#listen() {
		if (this.#listening || typeof window === 'undefined') return;
		this.#listening = true;
		window.addEventListener('keydown', this.#onKeydown, { capture: true });
	}

	#onKeydown = (event: KeyboardEvent) => {
		// Mid-composition (IME) keystrokes belong to the input method.
		if (event.isComposing || event.keyCode === 229) return;

		const chord = chordOf(event);
		const inText = isTextEntry(event.target);

		for (const s of this.#shortcuts) {
			if (normalize(s.keys) !== chord) continue;

			const hasMod = normalize(s.keys).startsWith('mod');
			const allowed = s.allowInInput ?? hasMod;
			if (inText && !allowed) continue;

			if (!s.passive) event.preventDefault();
			s.run(event);
			return;
		}
	};
}

export const shortcuts = new ShortcutRegistry();
