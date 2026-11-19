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

/** The assistant's name until you give it one, as the server has it
 *  (`assistant_profile::DEFAULT_ASSISTANT_NAME`). */
export const DEFAULT_ASSISTANT_NAME = 'Virtues';

export type SectionId = 'you' | 'assistant' | 'tools' | 'spending' | 'data' | 'appearance' | 'billing' | 'server';

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
	{ id: 'you', title: 'Profile', lede: 'The basics about you, which {name} and your pages use.', route: '/virtues/profile' },
	{ id: 'assistant', title: 'How {name} talks to you', lede: 'Its name and how it speaks to you.', route: '/virtues/assistant' },
	{ id: 'tools', title: 'What {name} can do', lede: "Turn off anything you don't want it to do.", route: '/virtues/assistant/tools' },
	{ id: 'spending', title: 'Models and spending', lede: 'Which models it uses and how much it may spend.', route: '/virtues/assistant/spending' },
	{ id: 'data', title: 'Your data', lede: 'How long your server keeps recordings, and a copy of everything.', route: '/virtues/data' },
	{ id: 'appearance', title: 'Appearance', lede: 'How the app looks and how your pages read.', route: '/virtues/appearance' },
	{ id: 'billing', title: 'Billing', lede: "Your subscription, your balance, and what you've paid.", route: '/virtues/billing' },
	{ id: 'server', title: 'Updates and network', lede: 'Keeping your server current, and reaching it when you are away.', route: '/virtues/server' },
];

export const SETTINGS: readonly Setting[] = [
	// You
	{ id: 'full-name', title: 'Your name', section: 'you', kind: 'control', words: ['full name', 'profile', 'me'], route: '/virtues/profile' },
	{ id: 'preferred-name', title: 'What {name} calls you', section: 'you', kind: 'control', words: ['preferred name', 'nickname', 'call me'], route: '/virtues/profile' },
	{ id: 'birthday', title: 'Birthday', section: 'you', kind: 'control', words: ['birth date', 'age', 'born'], route: '/virtues/profile' },
	{ id: 'occupation', title: 'Work', section: 'you', kind: 'control', words: ['occupation', 'job', 'role'], route: '/virtues/profile' },
	{ id: 'employer', title: 'Where you work', section: 'you', kind: 'control', words: ['employer', 'company', 'job'], route: '/virtues/profile' },
	{ id: 'home', title: 'Location', section: 'you', kind: 'control', words: ['home', 'time zone', 'timezone', 'city', 'where you live'], route: '/virtues/profile' },
	{ id: 'units', title: 'Units', section: 'you', kind: 'control', words: ['miles', 'kilometres', 'kilometers', 'km', 'fahrenheit', 'celsius', 'metric', 'imperial', 'pounds', 'temperature', 'distance'], route: '/virtues/profile' },

	// The assistant
	{ id: 'assistant-name', title: "{name}'s name", section: 'assistant', kind: 'control', words: ['assistant', 'rename', 'call it'], route: '/virtues/assistant' },
	{ id: 'style', title: 'How {name} talks to you', section: 'assistant', kind: 'control', words: ['style', 'tone', 'shorter', 'longer', 'answers', 'voice', 'instructions'], route: '/virtues/assistant' },
	{ id: 'subjects', title: 'Subjects {name} never raises', section: 'assistant', kind: 'control', words: ['rules', 'avoid', 'never bring up', 'topics'], route: '/virtues/assistant' },

	// What the assistant can do
	{ id: 'capabilities', title: 'Tools {name} can use', section: 'tools', kind: 'control', words: ['tools', 'web search', 'browser', 'applets', 'pages', 'permissions', 'allow', 'turn off', 'what it can do'], route: '/virtues/assistant/tools' },
	{ id: 'suggested-questions', title: 'Suggested questions', section: 'tools', kind: 'control', words: ['suggestions', 'prompts', 'new chat', 'ideas', 'speaking up'], route: '/virtues/assistant/tools' },

	// Models and spending
	{ id: 'models', title: 'Models {name} runs on', section: 'spending', kind: 'control', words: ['model', 'ai', 'chat model', 'background', 'faster', 'cheaper', 'smarter'], route: '/virtues/assistant/spending' },
	{ id: 'model-catalog', title: 'Every model you can pick', section: 'spending', kind: 'page', words: ['models', 'catalog', 'compare'], route: '/virtues/assistant/spending/models' },
	{ id: 'limit-day', title: 'Most tasks can spend in a day', section: 'spending', kind: 'control', words: ['spending limit', 'daily limit', 'cap', 'budget', 'spend', 'request'], route: '/virtues/assistant/spending' },
	{ id: 'limit-run', title: 'Most one task can spend', section: 'spending', kind: 'control', words: ['spending limit', 'per run', 'cap', 'budget', 'spend', 'request'], route: '/virtues/assistant/spending' },
	{ id: 'ai-cost', title: 'What AI has cost', section: 'spending', kind: 'page', words: ['usage', 'spend', 'cost', 'calls', 'spending'], route: '/virtues/assistant/spending/cost' },
	{ id: 'byo-ai', title: 'Your own AI provider', section: 'spending', kind: 'page', words: ['own key', 'bring your own', 'provider', 'endpoint', 'openai', 'openrouter', 'ollama', 'byo'], route: '/virtues/assistant/spending/provider' },

	// Your data
	{ id: 'keys', title: 'Keys', section: 'data', kind: 'page', words: ['api key', 'secrets', 'tokens', 'credentials'], route: '/virtues/keys' },
	{ id: 'shared-links', title: 'Shared links', section: 'data', kind: 'page', words: ['share', 'public', 'links', 'turn off'], route: '/virtues/shared' },
	{ id: 'backup', title: 'Backup', section: 'data', kind: 'page', words: ['backup', 'drive', 'restore', 'copy', 'recovery key', 'disaster'], route: '/virtues/backup' },
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
	{ id: 'invoices', title: 'Payments', section: 'billing', kind: 'action', words: ['invoice', 'invoices', 'receipt', 'receipts', 'charges', 'paid'], route: '/virtues/billing' },

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
 *  was chosen. A title that starts with it keeps a capital, and a name that
 *  ends in s takes a bare apostrophe ("Virtues' name"). */
export function titleOf(text: string, assistantName?: string | null): string {
	const name = assistantName?.trim() || 'your assistant';
	return text.replace(/\{name\}('s)?/g, (_m, possessive: string | undefined, offset: number) => {
		const shown = offset === 0 ? name.charAt(0).toUpperCase() + name.slice(1) : name;
		if (!possessive) return shown;
		return /s$/i.test(shown) ? `${shown}'` : `${shown}'s`;
	});
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
 * your server. `title` labels the group's line on the landing page and names
 * it in crumbs and search results, so a page is under the same name in all
 * three.
 */
export const LANDING: readonly { title: string; links: readonly LandingLink[] }[] = [
	{
		title: 'Your assistant',
		links: [
			{ label: 'How it talks', route: '/virtues/assistant' },
			{ label: 'What it can do', route: '/virtues/assistant/tools' },
			{ label: 'Models and spending', route: '/virtues/assistant/spending' },
		],
	},
	{
		title: 'Your profile',
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
			{ label: 'Your data', route: '/virtues/data' },
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
