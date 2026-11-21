/**
 * The mark each source and featured tool shows on the Sources page.
 *
 * A brand mark keeps its brand's color, because the mark is how people find a
 * service in a list. Marks that are black in their own brand (Notion, X) use
 * the text ink, so they stay visible in Dark (Notion, X, GitHub, Amazon). Sources without a brand mark
 * (devices, banks, past chats) fall back to the catalog's own icon in ink.
 *
 * The marks register here rather than in lib/icons.ts, so they load with the
 * Sources page instead of before the app's first paint. Notion is registered
 * there already, because pages outside Sources use it.
 */

import { addIcon } from '@iconify/svelte';
import siAmazon from '@iconify-icons/simple-icons/amazon';
import siDiscord from '@iconify-icons/simple-icons/discord';
import siGithub from '@iconify-icons/simple-icons/github';
import siGoogle from '@iconify-icons/simple-icons/google';
import siInstagram from '@iconify-icons/simple-icons/instagram';
import siLinear from '@iconify-icons/simple-icons/linear';
import siOutlook from '@iconify-icons/simple-icons/microsoftoutlook';
import siSlack from '@iconify-icons/simple-icons/slack';
import siSpotify from '@iconify-icons/simple-icons/spotify';
import siStrava from '@iconify-icons/simple-icons/strava';
import siTodoist from '@iconify-icons/simple-icons/todoist';
import siX from '@iconify-icons/simple-icons/x';

addIcon('simple-icons:amazon', siAmazon);
addIcon('simple-icons:discord', siDiscord);
addIcon('simple-icons:github', siGithub);
addIcon('simple-icons:google', siGoogle);
addIcon('simple-icons:instagram', siInstagram);
addIcon('simple-icons:linear', siLinear);
addIcon('simple-icons:microsoftoutlook', siOutlook);
addIcon('simple-icons:slack', siSlack);
addIcon('simple-icons:spotify', siSpotify);
addIcon('simple-icons:strava', siStrava);
addIcon('simple-icons:todoist', siTodoist);
addIcon('simple-icons:x', siX);

export interface Mark {
	icon: string;
	/** A CSS color. Absent means the text ink. */
	color?: string;
	/** A featured tool's line, used in place of the registry's description,
	 *  which is written by the publisher and often reads like an ad. */
	about?: string;
}

const SOURCE_MARKS: Record<string, Mark> = {
	amazon: { icon: 'simple-icons:amazon' },
	discord: { icon: 'simple-icons:discord', color: '#5865F2' },
	github: { icon: 'simple-icons:github' },
	google: { icon: 'simple-icons:google', color: '#4285F4' },
	instagram: { icon: 'simple-icons:instagram', color: '#E4405F' },
	notion: { icon: 'simple-icons:notion' },
	outlook: { icon: 'simple-icons:microsoftoutlook', color: '#0078D4' },
	slack: { icon: 'simple-icons:slack', color: '#4A154B' },
	spotify: { icon: 'simple-icons:spotify', color: '#1DB954' },
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
