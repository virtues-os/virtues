/**
 * One entry in an insert menu (`SlashMenu`), whichever editor fills it: the
 * CodeMirror page editor's `SlashCommand` and the block editor's
 * `TreeCommand` both extend this, so the menu and its filter are shared.
 */

export interface MenuCommand {
	label: string;
	/** Extra terms the label alone would not match ("h1", "todo", "hr"). */
	keywords?: string[];
	icon: string;
	/**
	 * Left out of the unfiltered list and shown once a query matches it: for
	 * the rare entries (Heading 4 to 6) that would make the short list long.
	 */
	searchOnly?: boolean;
}

/** The commands a query matches, in the order given. */
export function filterCommands<T extends MenuCommand>(commands: T[], query: string): T[] {
	if (!query) return commands.filter((cmd) => !cmd.searchOnly);
	const q = query.toLowerCase();
	return commands.filter(
		(cmd) => cmd.label.toLowerCase().includes(q) || cmd.keywords?.some((k) => k.includes(q)),
	);
}

/** What a key does in an open insert menu: close it, reach the next or the previous command, or run the one reached. */
export type MenuKey = 'close' | 'next' | 'previous' | 'run';

/**
 * What `key` does in an open insert menu listing `count` commands; null
 * leaves it to the editor. With no command to reach or run, Enter, Tab and
 * the arrows are the editor's: a `/` in a path (`see /etc/hosts`) opens the
 * menu, which matches nothing, and Enter still ends the line.
 */
export function menuKey(key: string, count: number): MenuKey | null {
	if (key === 'Escape') return 'close';
	if (!count) return null;
	switch (key) {
		case 'ArrowDown':
			return 'next';
		case 'ArrowUp':
			return 'previous';
		case 'Enter':
		case 'Tab':
			return 'run';
		default:
			return null;
	}
}
