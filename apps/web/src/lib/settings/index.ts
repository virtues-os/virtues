/**
 * Every setting in Settings, grouped by the page it lives on.
 *
 * Settings opens on a landing page (settings/SettingsLanding.svelte): the
 * search field, what needs you, Changed lately, and a link to each page
 * (`LANDING`). Each section is a page of its own (settings/SettingsSection
 * .svelte) at its `route`. This list is what the pages, the search field
 * and the ⌘K palette all read, so a setting is added, renamed or moved here
 * once. Each entry says what the setting is called on its own, the words
 * people type for it, which section it belongs to, what kind of row it is,
 * and the route that shows it.
 *
 * Kinds:
 *   - control: something you change in the row (a field, a switch, a choice);
 *   - value:   something you read, with a link to more;
 *   - action:  a one-off you start (check for updates, pair, start over).
 *              Search never runs one; it opens the page, which asks first;
 *   - page:    a row that opens its own page, because what it holds is a
 *              list, a chart, a table or a form of more than a few fields.
 *
 * Only settings the app has today are listed. A setting the plan adds joins
 * the list in the change that builds it.
 */
import { matchRank } from '$lib/search/match';

export type SectionId = 'you' | 'assistant' | 'data' | 'appearance' | 'billing' | 'server';

export interface Section {
	id: SectionId;
	/** The page's title. `{name}` is the assistant's name (`titleOf`). */
	title: string;
	/** One sentence under the title. */
	lede: string;
	/** The section's own page, or null for one whose settings each have a
	 *  page (Your data). */
	route: string | null;
}

export type SettingKind = 'control' | 'value' | 'action' | 'page';

export interface Setting {
	/** Stable id, used as the row's anchor on its page (`setting-<id>`). */
	id: string;
	/** What the setting is called when it stands alone, as in a search
	 *  result. Sentence case. `{name}` is the assistant's name. */
	title: string;
	section: SectionId;
	kind: SettingKind;
	/** The words people type for it, beyond those in the title. */
	words: string[];
	/** Where it lives: its section's page, or the page a row opens. */
	route: string;
	/** Undoing it is not possible (unpair everything, start over). */
	destructive?: boolean;
}

export const SECTIONS: readonly Section[] = [
	{ id: 'you', title: 'Profile', lede: 'What {name} knows about you to start with. Your pages and day summaries use it too.', route: '/virtues/profile' },
	{ id: 'assistant', title: '{name}', lede: 'How it talks to you, what it can do, and the models it runs on.', route: '/virtues/assistant' },
	{ id: 'data', title: 'What {name} may read', lede: 'Which of your records {name} sees, how long your server keeps recordings, and a copy of everything.', route: '/virtues/data' },
	{ id: 'appearance', title: 'Appearance', lede: 'How the app looks, how your pages read, and how much it moves.', route: '/virtues/appearance' },
	{ id: 'billing', title: 'Billing', lede: 'Your subscription, what AI may spend, and what it has cost.', route: '/virtues/billing' },
	{ id: 'server', title: 'Updates and network', lede: 'Keeping your server current, and reaching it when you are away.', route: '/virtues/server' },
];

export const SETTINGS: readonly Setting[] = [
	// You
	{ id: 'full-name', title: 'Your name', section: 'you', kind: 'control', words: ['full name', 'profile', 'me'], route: '/virtues/profile' },
	{ id: 'preferred-name', title: 'What {name} calls you', section: 'you', kind: 'control', words: ['preferred name', 'nickname', 'call me'], route: '/virtues/profile' },
	{ id: 'birthday', title: 'Birthday', section: 'you', kind: 'control', words: ['birth date', 'age', 'born'], route: '/virtues/profile' },
	{ id: 'occupation', title: 'Work', section: 'you', kind: 'control', words: ['occupation', 'job', 'role'], route: '/virtues/profile' },
	{ id: 'employer', title: 'Where you work', section: 'you', kind: 'control', words: ['employer', 'company', 'job'], route: '/virtues/profile' },
	{ id: 'home', title: 'Where you live', section: 'you', kind: 'control', words: ['home', 'time zone', 'timezone', 'city', 'location'], route: '/virtues/profile' },
	{ id: 'units', title: 'Units', section: 'you', kind: 'control', words: ['miles', 'kilometres', 'kilometers', 'km', 'fahrenheit', 'celsius', 'metric', 'imperial', 'pounds', 'temperature', 'distance'], route: '/virtues/profile' },

	// The assistant
	{ id: 'assistant-name', title: "{name}'s name", section: 'assistant', kind: 'control', words: ['assistant', 'rename', 'call it'], route: '/virtues/assistant' },
	{ id: 'style', title: 'How {name} talks to you', section: 'assistant', kind: 'control', words: ['style', 'tone', 'shorter', 'longer', 'answers', 'voice', 'instructions'], route: '/virtues/assistant' },
	{ id: 'subjects', title: 'Subjects {name} never raises', section: 'assistant', kind: 'control', words: ['rules', 'avoid', 'never bring up', 'topics'], route: '/virtues/assistant' },
	{ id: 'capabilities', title: 'What {name} can do', section: 'assistant', kind: 'control', words: ['tools', 'web search', 'browser', 'applets', 'pages', 'permissions', 'allow', 'turn off'], route: '/virtues/assistant' },
	{ id: 'models', title: 'Models {name} runs on', section: 'assistant', kind: 'control', words: ['model', 'ai', 'chat model', 'background', 'faster', 'cheaper', 'smarter'], route: '/virtues/assistant' },
	{ id: 'model-catalog', title: 'Every model you can pick', section: 'assistant', kind: 'page', words: ['models', 'catalog', 'compare'], route: '/virtues/assistant/models' },
	{ id: 'suggested-questions', title: 'Suggested questions', section: 'assistant', kind: 'control', words: ['suggestions', 'prompts', 'new chat', 'ideas'], route: '/virtues/assistant' },

	// Your data
	{ id: 'memory', title: 'What {name} remembers', section: 'data', kind: 'page', words: ['memory', 'memories', 'notes', 'forget'], route: '/virtues/assistant/memory' },
	{ id: 'keys', title: 'Keys', section: 'data', kind: 'page', words: ['api key', 'secrets', 'tokens', 'credentials'], route: '/virtues/keys' },
	{ id: 'shared-links', title: 'Shared links', section: 'data', kind: 'page', words: ['share', 'public', 'links', 'turn off'], route: '/virtues/shared' },
	{ id: 'backup', title: 'Backup', section: 'data', kind: 'page', words: ['backup', 'drive', 'restore', 'copy', 'recovery key', 'disaster'], route: '/virtues/backup' },
	{ id: 'model-reading', title: 'What {name} may read', section: 'data', kind: 'control', words: ['privacy', 'access', 'hide', 'messages', 'health', 'location', 'money', 'cycle', 'period', 'see my'], route: '/virtues/data' },
	{ id: 'keep-recordings', title: 'Keep recordings', section: 'data', kind: 'control', words: ['audio', 'delete recordings', 'retention', 'microphone', 'voice', 'how long'], route: '/virtues/data' },
	{ id: 'export', title: 'Export everything', section: 'data', kind: 'action', words: ['download', 'copy', 'takeout', 'leave', 'my data', 'json'], route: '/virtues/data' },

	// Appearance
	{ id: 'theme', title: 'Theme', section: 'appearance', kind: 'control', words: ['dark', 'light', 'mode', 'color', 'colour', 'appearance', 'look'], route: '/virtues/appearance' },
	{ id: 'accent', title: 'Accent', section: 'appearance', kind: 'control', words: ['accent', 'color', 'colour', 'tint', 'highlight', 'brand', 'personalize'], route: '/virtues/appearance' },
	{ id: 'glass', title: 'Glass', section: 'appearance', kind: 'control', words: ['glass', 'transparency', 'translucent', 'blur', 'see-through', 'solid', 'frosted'], route: '/virtues/appearance' },
	{ id: 'reading-font', title: 'Reading font', section: 'appearance', kind: 'control', words: ['font', 'typeface', 'serif', 'sans', 'mono', 'pages', 'reading'], route: '/virtues/appearance' },
	{ id: 'text-size', title: 'Text size', section: 'appearance', kind: 'control', words: ['font size', 'bigger text', 'smaller text', 'larger', 'zoom', 'readable'], route: '/virtues/appearance' },
	{ id: 'reduce-motion', title: 'Reduce motion', section: 'appearance', kind: 'control', words: ['animation', 'motion', 'movement', 'still', 'accessibility', 'dizzy'], route: '/virtues/appearance' },

	// Billing
	{ id: 'subscription', title: 'Subscription', section: 'billing', kind: 'action', words: ['plan', 'stripe', 'payment', 'card', 'cancel', 'renew'], route: '/virtues/billing' },
	{ id: 'balance', title: 'Balance', section: 'billing', kind: 'value', words: ['wallet', 'credit', 'money', 'left'], route: '/virtues/billing' },
	{ id: 'auto-top-up', title: 'Auto top-up', section: 'billing', kind: 'control', words: ['top up', 'refill', 'add money', 'limit'], route: '/virtues/billing' },
	{ id: 'limit-day', title: 'Most a request can spend in a day', section: 'billing', kind: 'control', words: ['spending limit', 'daily limit', 'cap', 'budget', 'spend'], route: '/virtues/billing' },
	{ id: 'limit-run', title: 'Most one request can spend', section: 'billing', kind: 'control', words: ['spending limit', 'per run', 'cap', 'budget', 'spend'], route: '/virtues/billing' },
	{ id: 'byo-ai', title: 'Bring your own AI', section: 'billing', kind: 'page', words: ['own key', 'provider', 'endpoint', 'openai', 'byo'], route: '/virtues/billing/byo' },
	{ id: 'ai-cost', title: 'What AI has cost', section: 'billing', kind: 'page', words: ['usage', 'spend', 'cost', 'calls', 'spending'], route: '/virtues/billing/cost' },

	// Server and devices
	{ id: 'updates', title: 'Check for updates', section: 'server', kind: 'action', words: ['update', 'version', 'upgrade', 'release'], route: '/virtues/server' },
	{ id: 'auto-install', title: 'Install updates on their own', section: 'server', kind: 'control', words: ['automatic updates', 'auto update'], route: '/virtues/server' },
	{ id: 'update-channel', title: 'Which updates', section: 'server', kind: 'control', words: ['channel', 'early', 'beta', 'released', 'prerelease'], route: '/virtues/server' },
	{ id: 'reach', title: 'Reach your server away from home', section: 'server', kind: 'control', words: ['remote', 'relay', 'network', 'anywhere', 'away'], route: '/virtues/server' },
	{ id: 'search-runs-on', title: 'Where search runs', section: 'server', kind: 'page', words: ['search', 'accelerator', 'npu', 'cpu', 'index'], route: '/virtues/search' },
	{ id: 'server-health', title: 'How your server is doing', section: 'server', kind: 'page', words: ['system', 'storage', 'disk', 'memory', 'cpu', 'health'], route: '/virtues/system' },
	{ id: 'devices', title: 'Devices', section: 'server', kind: 'page', words: ['phone', 'iphone', 'mac', 'paired', 'unpair'], route: '/virtues/devices' },
	{ id: 'pair-device', title: 'Pair a device', section: 'server', kind: 'action', words: ['add device', 'connect', 'new phone'], route: '/virtues/devices' },
	{ id: 'start-over', title: 'Start over', section: 'server', kind: 'action', words: ['reset', 'unpair everything', 'setup again'], route: '/virtues/server', destructive: true },
];

/** The assistant's name in a title or lede, or "your assistant" where none
 *  was chosen. A title that starts with it keeps a capital. */
export function titleOf(text: string, assistantName?: string | null): string {
	const name = assistantName?.trim() || 'your assistant';
	return text.replace(/\{name\}/g, (_m, offset: number) =>
		offset === 0 ? name.charAt(0).toUpperCase() + name.slice(1) : name,
	);
}

export function findSetting(id: string): Setting | undefined {
	return SETTINGS.find((s) => s.id === id);
}

export function settingsIn(section: SectionId): Setting[] {
	return SETTINGS.filter((s) => s.section === section);
}

/**
 * The settings that match what was typed, best first. A setting matches on
 * its title (with the assistant's name filled in) or any of its words, and
 * keeps its best rank; ties keep the page's order.
 */
export function matchSettings(query: string, assistantName?: string | null): Setting[] {
	const q = query.trim();
	if (!q) return [...SETTINGS];
	return SETTINGS.map((setting, order) => {
		const names = [titleOf(setting.title, assistantName), ...setting.words];
		const rank = Math.max(...names.map((n) => matchRank(n, q)));
		return { setting, rank, order };
	})
		.filter((x) => x.rank > 0)
		.sort((a, b) => b.rank - a.rank || a.order - b.order)
		.map((x) => x.setting);
}

/** One link in the landing page's site map. */
export interface LandingLink {
	label: string;
	/** The page it opens. */
	route: string;
	/** A row on that page to light when it opens, for a link that names one
	 *  part of a section page (Voice is a row on the assistant's page). */
	setting?: string;
}

/**
 * The landing page's site map, one line per party: the assistant, you, and
 * your server. The landing page shows your name in place of "You".
 */
export const LANDING: readonly { title: string; links: readonly LandingLink[] }[] = [
	{
		title: 'Assistant',
		links: [
			{ label: 'Voice', route: '/virtues/assistant', setting: 'style' },
			{ label: 'Data access', route: '/virtues/data' },
			{ label: 'Memory', route: '/virtues/assistant/memory' },
			{ label: 'Tools and models', route: '/virtues/assistant', setting: 'capabilities' },
			{ label: 'Spending limits', route: '/virtues/billing', setting: 'limit-run' },
		],
	},
	{
		title: 'You',
		links: [
			{ label: 'Profile', route: '/virtues/profile' },
			{ label: 'Appearance', route: '/virtues/appearance' },
			{ label: 'Billing', route: '/virtues/billing' },
		],
	},
	{
		title: 'Your server',
		links: [
			{ label: 'Status', route: '/virtues/system' },
			{ label: 'Devices', route: '/virtues/devices' },
			{ label: 'Updates and network', route: '/virtues/server' },
			{ label: 'Backup', route: '/virtues/backup' },
			{ label: 'Keys', route: '/virtues/keys' },
			{ label: 'Shared links', route: '/virtues/shared' },
		],
	},
];

/**
 * The page a setting lives on, as Settings names it: its section's page
 * ("Billing"), else the landing group that links to it ("Your server"), else
 * Settings itself.
 */
export function settingPlace(setting: Setting, assistantName?: string | null): string {
	const section = SECTIONS.find((s) => s.id === setting.section);
	if (section?.route && (setting.route === section.route || setting.route.startsWith(`${section.route}/`))) {
		return titleOf(section.title, assistantName);
	}
	const group = LANDING.find((g) => g.links.some((l) => l.route === setting.route));
	if (group) return group.title;
	return section ? titleOf(section.title, assistantName) : 'Settings';
}

/** The section whose page is `route`, if any. */
export function sectionAt(route: string): Section | undefined {
	return SECTIONS.find((s) => s.route === route);
}
