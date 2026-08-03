/**
 * Page display settings — the "Aa" popover model.
 *
 * One persisted, app-wide set of reading/writing preferences for the page
 * editor: font mode, text size, page width, and focus/typewriter mode.
 * Font + size are surfaced to CodeMirror via the `--editor-font-family` /
 * `--editor-font-size` custom properties (see codemirror/theme.ts), set on an
 * ancestor of the editor so they cascade in.
 */

export type FontMode = "sans" | "serif" | "mono";
export type TextSize = "s" | "m" | "l";
export type WidthMode = "small" | "medium" | "full";

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

// Line-height tracks the text size: tighter for small (denser reading), airier
// for large (more breathing room). Surfaced as `--editor-line-height`.
const LINE_HEIGHT: Record<TextSize, string> = {
	s: "1.55",
	m: "1.7",
	l: "1.8",
};

interface Persisted {
	fontMode: FontMode;
	textSize: TextSize;
	widthMode: WidthMode;
	focusMode: boolean;
	spellcheck: boolean;
	rawMode: boolean;
}

const DEFAULTS: Persisted = {
	fontMode: "sans",
	textSize: "m",
	widthMode: "medium",
	focusMode: false,
	spellcheck: true,
	rawMode: false,
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
	/**
	 * Show the literal markdown instead of the rendered surface. The escape
	 * hatch for when a construct mis-parses — see codemirror/extensions/render-mode.ts.
	 */
	rawMode = $state(DEFAULTS.rawMode);

	constructor() {
		const p = load();
		this.fontMode = p.fontMode;
		this.textSize = p.textSize;
		this.widthMode = p.widthMode;
		this.focusMode = p.focusMode;
		this.spellcheck = p.spellcheck;
		this.rawMode = p.rawMode;
	}

	/** CSS value for `--editor-font-family`. */
	get fontFamily(): string {
		return FONT_FAMILY[this.fontMode];
	}

	/** CSS value for `--editor-font-size`. */
	get fontSize(): string {
		return FONT_SIZE[this.textSize];
	}

	/** CSS value for `--editor-line-height`. Tracks the text size. */
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
					rawMode: this.rawMode,
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
		const modes: WidthMode[] = ["small", "medium", "full"];
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

	toggleRaw() {
		this.rawMode = !this.rawMode;
		this.persist();
	}
}

export const pageDisplay = new PageDisplay();
