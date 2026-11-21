/**
 * The mark each source and featured tool shows on the Sources page.
 *
 * A brand mark keeps its brand's color, because the mark is how people find a
 * service in a list. Marks that are black in their own brand (Notion, X) use
 * the text ink, so they stay visible in Dark (Notion, X, GitHub). Sources without a brand mark
 * (devices, banks, past chats) fall back to the catalog's own icon in ink.
 */

export interface Mark {
	icon: string;
	/** A CSS color. Absent means the text ink. */
	color?: string;
	/** A featured tool's line, used in place of the registry's description,
	 *  which is written by the publisher and often reads like an ad. */
	about?: string;
}

const SOURCE_MARKS: Record<string, Mark> = {
	github: { icon: 'simple-icons:github' },
	google: { icon: 'simple-icons:google', color: '#4285F4' },
	instagram: { icon: 'simple-icons:instagram', color: '#E4405F' },
	notion: { icon: 'simple-icons:notion' },
	strava: { icon: 'simple-icons:strava', color: '#FC4C02' },
	x: { icon: 'simple-icons:x' }
};

/** Featured MCP servers, by registry name (`tools/connection_registry.rs`). */
const TOOL_MARKS: Record<string, Mark> = {
	'net.todoist/mcp': { icon: 'simple-icons:todoist', color: '#E44332', about: 'Adds and finishes tasks for you' },
	'app.linear/linear': { icon: 'simple-icons:linear', color: '#5E6AD2', about: 'Reads and files issues for you' }
};

export function sourceMark(id: string, catalogIcon: string | null): Mark {
	return SOURCE_MARKS[id] ?? { icon: catalogIcon ?? 'mynaui:power' };
}

export function toolMark(registry: string | null): Mark {
	return (registry && TOOL_MARKS[registry]) || { icon: 'mynaui:link' };
}
