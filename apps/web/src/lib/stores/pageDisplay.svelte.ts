/**
 * Page display settings — the "Aa" popover model.
 *
 * One persisted, app-wide set of reading/writing preferences for the page
 * editor: font mode, text size, page width, and focus/typewriter mode.
 * Width `page` sets the page as an A4 sheet (210 mm), the width it prints at.
 * Font + size are surfaced to the editor via the `--prose-family` /
 * `--prose-size` custom properties (see lib/styles/prose.css), set
 * on an ancestor of the editor so they cascade in.
 */

export type FontMode = "sans" | "serif" | "mono";
export type TextSize = "s" | "m" | "l";
export type WidthMode = "small" | "medium" | "full" | "page";

const STORAGE_KEY = "virtues-page-display";

const FONT_FAMILY: Record<FontMode, string> = {
	sans: "var(--font-sans, ui-sans-serif, system-ui, -apple-system, sans-serif)",
	serif: "var(--font-serif, Georgia, 'Times New Roman', serif)",
	mono: "var(--font-mono, ui-monospace, monospace)",
};

const FONT_SIZE: Record<TextSize, string> = {
	s: "0.9375rem",
	m: "1rem",
	l: "1.125rem",
};

// Line-height tracks the text size and lands on a whole pixel: 24px for small
// and medium, 28px for large. Medium is chat's body line (`--md-body-lh`), so
// a page and a reply read at one rhythm. Surfaced as `--prose-lh`.
const LINE_HEIGHT: Record<TextSize, string> = {
	s: "1.5rem",
	m: "var(--md-body-lh)",
	l: "1.75rem",
};

interface Persisted {
	fontMode: FontMode;
	textSize: TextSize;
	widthMode: WidthMode;
	focusMode: boolean;
	spellcheck: boolean;
}

const DEFAULTS: Persisted = {
	fontMode: "sans",
	textSize: "m",
	widthMode: "medium",
	focusMode: false,
	spellcheck: true,
};

function load(): Persisted {
	try {
		const saved = localStorage.getItem(STORAGE_KEY);
		if (saved) return { ...DEFAULTS, ...JSON.parse(saved) };
	} catch {
		// SSR or unavailable storage — fall through to defaults
	}
	return { ...DEFAULTS };
}

class PageDisplay {
	fontMode = $state<FontMode>(DEFAULTS.fontMode);
	textSize = $state<TextSize>(DEFAULTS.textSize);
	widthMode = $state<WidthMode>(DEFAULTS.widthMode);
	focusMode = $state(DEFAULTS.focusMode);
	spellcheck = $state(DEFAULTS.spellcheck);

	constructor() {
		const p = load();
		this.fontMode = p.fontMode;
		this.textSize = p.textSize;
		this.widthMode = p.widthMode;
		this.focusMode = p.focusMode;
		this.spellcheck = p.spellcheck;
	}

	/** CSS value for `--prose-family`. */
	get fontFamily(): string {
		return FONT_FAMILY[this.fontMode];
	}

	/** CSS value for `--prose-size`. */
	get fontSize(): string {
		return FONT_SIZE[this.textSize];
	}

	/** CSS value for `--prose-lh`. Tracks the text size. */
	get lineHeight(): string {
		return LINE_HEIGHT[this.textSize];
	}

	private persist() {
		try {
			localStorage.setItem(
				STORAGE_KEY,
				JSON.stringify({
					fontMode: this.fontMode,
					textSize: this.textSize,
					widthMode: this.widthMode,
					focusMode: this.focusMode,
					spellcheck: this.spellcheck,
				}),
			);
		} catch {
			// ignore
		}
	}

	setFontMode(mode: FontMode) {
		this.fontMode = mode;
		this.persist();
	}

	setTextSize(size: TextSize) {
		this.textSize = size;
		this.persist();
	}

	setWidth(mode: WidthMode) {
		this.widthMode = mode;
		this.persist();
	}

	cycleWidth() {
		const modes: WidthMode[] = ["small", "medium", "full", "page"];
		this.widthMode = modes[(modes.indexOf(this.widthMode) + 1) % modes.length];
		this.persist();
	}

	toggleFocus() {
		this.focusMode = !this.focusMode;
		this.persist();
	}

	toggleSpellcheck() {
		this.spellcheck = !this.spellcheck;
		this.persist();
	}
}

export const pageDisplay = new PageDisplay();
