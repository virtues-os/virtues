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

export type SectionId = 'you' | 'assistant' | 'tools' | 'spending' | 'data' | 'appearance' | 'billing' | 'server' | 'updates' | 'devices' | 'device';

/** A platform with settings of its own on This device. */
export type DevicePlatform = 'mac' | 'ios';

export interface Section {
	id: SectionId;
	/** The page's title. `{name}` is the assistant's name (`titleOf`). */
	title: string;
	/** One sentence under the title. */
	lede: string;
	/** The section's own page, or null for one whose settings each have a
	 *  page (Your server). */
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
	/** Only on this platform's This device page (pausing the Mac's collector,
	 *  the phone's notifications). Search lists it only there. */
	platform?: DevicePlatform;
}

export const SECTIONS: readonly Section[] = [
	{ id: 'you', title: 'Profile', lede: 'The basics about you, which {name} and your pages use.', route: '/virtues/profile' },
	{ id: 'assistant', title: 'How {name} talks to you', lede: 'Its name and how it speaks to you.', route: '/virtues/assistant' },
	{ id: 'tools', title: 'What {name} can do', lede: "Turn off anything you don't want it to do.", route: '/virtues/assistant/tools' },
	{ id: 'spending', title: 'Models and spending', lede: 'Which models it uses and how much it may spend.', route: '/virtues/assistant/spending' },
	{ id: 'data', title: 'Your data', lede: "Copies of your data, what your server keeps, and what you've shared.", route: '/virtues/data' },
	{ id: 'appearance', title: 'Appearance', lede: 'How the app looks and how your pages read.', route: '/virtues/appearance' },
	{ id: 'billing', title: 'Billing', lede: "Your subscription, your balance, and what you've paid.", route: '/virtues/billing' },
	{ id: 'server', title: 'Your server', lede: 'Whether your server is all right, what it runs, and the machine itself.', route: null },
	{ id: 'updates', title: 'Updates', lede: 'New releases of your server, and which ones it takes.', route: '/virtues/updates' },
	{ id: 'devices', title: 'Devices', lede: 'Everything paired to your server, and how each one reaches it.', route: '/virtues/devices' },
	// This device's line on the landing page appears only on a device with a
	// page of its own (deviceLanding), so the section has no fixed route.
	{ id: 'device', title: 'This device', lede: 'What only the device in your hand can show or change.', route: null },
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

	// Models and spending
	{ id: 'models', title: 'Models {name} runs on', section: 'spending', kind: 'control', words: ['model', 'ai', 'chat model', 'background', 'faster', 'cheaper', 'smarter'], route: '/virtues/assistant/spending' },
	{ id: 'model-catalog', title: 'Every model you can pick', section: 'spending', kind: 'page', words: ['models', 'catalog', 'compare'], route: '/virtues/assistant/spending/models' },
	{ id: 'limit-day', title: 'Most tasks can spend in a day', section: 'spending', kind: 'control', words: ['spending limit', 'daily limit', 'cap', 'budget', 'spend', 'request'], route: '/virtues/assistant/spending' },
	{ id: 'limit-run', title: 'Most one task can spend', section: 'spending', kind: 'control', words: ['spending limit', 'per run', 'cap', 'budget', 'spend', 'request'], route: '/virtues/assistant/spending' },
	{ id: 'ai-cost', title: 'What AI has cost', section: 'spending', kind: 'page', words: ['usage', 'spend', 'cost', 'calls', 'spending'], route: '/virtues/assistant/spending/cost' },
	{ id: 'byo-ai', title: 'Your own AI provider', section: 'spending', kind: 'page', words: ['own key', 'bring your own', 'provider', 'endpoint', 'openai', 'openrouter', 'ollama', 'byo'], route: '/virtues/assistant/spending/provider' },

	// Your data
	{ id: 'backup', title: 'Backup', section: 'data', kind: 'page', words: ['backup', 'drive', 'restore', 'copy', 'recovery key', 'disaster'], route: '/virtues/backup' },
	{ id: 'export', title: 'Export everything', section: 'data', kind: 'action', words: ['download', 'copy', 'takeout', 'leave', 'my data', 'json'], route: '/virtues/data' },
	{ id: 'keep-recordings', title: 'Keep recordings', section: 'data', kind: 'control', words: ['audio', 'delete recordings', 'retention', 'microphone', 'voice', 'how long'], route: '/virtues/data' },
	{ id: 'deleted-records', title: 'Deleted records', section: 'data', kind: 'page', words: ['delete', 'remove', 'erase', 'forget', 'purchase', 'visit', 'record', 'delete-data', 'tombstone', 'collect again', 'for good'], route: '/virtues/data/deleted' },
	{ id: 'shared-links', title: 'Shared links', section: 'data', kind: 'page', words: ['share', 'public', 'links', 'turn off'], route: '/virtues/data/shared' },
	{ id: 'keys', title: 'Keys', section: 'data', kind: 'page', words: ['api key', 'secrets', 'tokens', 'credentials'], route: '/virtues/data/keys' },

	// Appearance
	{ id: 'theme', title: 'Theme', section: 'appearance', kind: 'control', words: ['dark', 'light', 'mode', 'color', 'colour', 'appearance', 'look', 'night', 'sunset', 'sunrise'], route: '/virtues/appearance' },
	{ id: 'background', title: 'Background', section: 'appearance', kind: 'control', words: ['background', 'picture', 'painting', 'wallpaper', 'image', 'home', 'lake', 'plain'], route: '/virtues/appearance' },
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

	// Your server
	{ id: 'server-health', title: 'Your server', section: 'server', kind: 'page', words: ['status', 'system', 'storage', 'disk', 'memory', 'cpu', 'health', 'temperature'], route: '/virtues/system' },
	{ id: 'search-runs-on', title: 'Where search runs', section: 'server', kind: 'page', words: ['search', 'accelerator', 'npu', 'cpu', 'index'], route: '/virtues/search' },
	{ id: 'applets', title: 'Applets', section: 'server', kind: 'page', words: ['scheduled', 'background', 'jobs', 'runs', 'failed'], route: '/virtues/applets' },

	// Updates
	{ id: 'updates', title: 'Check for updates', section: 'updates', kind: 'action', words: ['update', 'version', 'upgrade', 'release', 'install'], route: '/virtues/updates' },
	{ id: 'auto-install', title: 'Install updates automatically', section: 'updates', kind: 'control', words: ['automatic updates', 'auto update', 'on their own'], route: '/virtues/updates' },
	{ id: 'update-channel', title: 'Which updates', section: 'updates', kind: 'control', words: ['channel', 'early', 'beta', 'released', 'prerelease'], route: '/virtues/updates' },

	// Devices
	{ id: 'pair-device', title: 'Pair a device', section: 'devices', kind: 'action', words: ['add device', 'connect', 'new phone', 'devices', 'paired', 'agent'], route: '/virtues/devices' },
	{ id: 'devices', title: 'Unpair a device', section: 'devices', kind: 'action', words: ['devices', 'remove device', 'revoke', 'disconnect', 'lost phone'], route: '/virtues/devices' },
	{ id: 'reach', title: 'Reach your server away from home', section: 'devices', kind: 'control', words: ['remote', 'relay', 'network', 'anywhere', 'away'], route: '/virtues/devices' },
	{ id: 'start-over', title: 'Start over', section: 'devices', kind: 'action', words: ['reset', 'unpair everything', 'setup again'], route: '/virtues/devices', destructive: true },

	// This device
	{ id: 'this-device', title: 'This device', section: 'device', kind: 'page', words: ['this mac', 'this phone', 'this computer', 'app version'], route: '/virtues/devices/this' },
	{ id: 'collecting', title: 'Collecting on this Mac', section: 'device', kind: 'action', words: ['pause', 'resume', 'collector', 'stop collecting'], route: '/virtues/devices/this', platform: 'mac' },
	{ id: 'permissions', title: 'What this Mac may read', section: 'device', kind: 'value', words: ['permissions', 'full disk access', 'accessibility', 'microphone', 'system settings'], route: '/virtues/devices/this', platform: 'mac' },
	{ id: 'what-it-sends', title: 'What this phone sends', section: 'device', kind: 'value', words: ['location', 'health', 'contacts', 'calendar', 'finance', 'audio', 'streams'], route: '/virtues/devices/this', platform: 'ios' },
	{ id: 'notifications', title: 'Notifications on this phone', section: 'device', kind: 'value', words: ['push', 'notify', 'alerts'], route: '/virtues/devices/this', platform: 'ios' },
	{ id: 'the-app', title: 'This app', section: 'device', kind: 'value', words: ['app version', 'app update', 'relaunch'], route: '/virtues/devices/this' },
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
export function matchSettings(query: string, assistantName?: string | null, platform?: DevicePlatform | null): Setting[] {
	const here = SETTINGS.filter((s) => !s.platform || s.platform === platform);
	const q = query.trim();
	if (!q) return here;
	return here.map((setting, order) => {
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

/** One line of the landing page's site map. */
export interface LandingGroup {
	/** Labels the line on the landing page and names it in crumbs and search
	 *  results, so a page is under the same name in all three. */
	title: string;
	/** The page the title opens, for a party with a page of its own: your
	 *  server, this device. */
	route?: string;
	links: readonly LandingLink[];
}

/**
 * The landing page's site map, one line per party: the assistant, you, and
 * your server. This device joins them where there is one (`deviceLanding`).
 */
export const LANDING: readonly LandingGroup[] = [
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
		route: '/virtues/system',
		links: [
			{ label: 'Devices', route: '/virtues/devices' },
			{ label: 'Updates', route: '/virtues/updates' },
			{ label: 'Backup', route: '/virtues/backup' },
			{ label: 'Your data', route: '/virtues/data' },
		],
	},
];

/** The kind of device the app is running on, as far as This device is concerned. */
export type DeviceHere =
	/** The Mac app: its collector, live permissions and its own updates. */
	| 'mac-app'
	/** The iPhone or iPad app. */
	| 'ios-app'
	/** The Windows or Linux app, which only shows the server's screens. */
	| 'desktop-app'
	/** A browser on a paired computer, which counts as that computer. */
	| 'browser';

/**
 * This device's line on the landing page. Its links name the parts of the
 * page only that device can show. A browser on the server itself (the
 * console) has no line: Your server already covers that machine.
 */
export function deviceLanding(here: DeviceHere): LandingGroup {
	const route = '/virtues/devices/this';
	const links: Record<DeviceHere, LandingLink[]> = {
		'mac-app': [
			{ label: 'Collecting', route, setting: 'collecting' },
			{ label: 'Permissions', route, setting: 'permissions' },
			{ label: 'The app', route, setting: 'the-app' },
		],
		'ios-app': [
			{ label: 'What it sends', route, setting: 'what-it-sends' },
			{ label: 'Notifications', route, setting: 'notifications' },
			{ label: 'The app', route, setting: 'the-app' },
		],
		'desktop-app': [{ label: 'The app', route, setting: 'the-app' }],
		browser: [
			{ label: 'Permissions', route, setting: 'permissions' },
			{ label: 'The app', route, setting: 'the-app' },
		],
	};
	return { title: 'This device', route, links: links[here] };
}

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
