/**
 * Every setting in Settings, in the order the page shows them.
 *
 * Settings is one scrolling page in six sections
 * (agents/plan/settings-search-plan.md). This list is what that page, its
 * search field and the ⌘K palette all read, so a setting is added, renamed or
 * moved here once. Each entry says what the setting is called on its own, the
 * words people type for it, which section it belongs to, what kind of row it
 * is, and the route that renders it today.
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
	/** The heading. `{name}` is the assistant's name (`titleOf`). */
	title: string;
	/** One sentence under the heading. */
	lede: string;
}

export type SettingKind = 'control' | 'value' | 'action' | 'page';

export interface Setting {
	/** Stable id, used as the row's anchor on the page (`/virtues#<id>`). */
	id: string;
	/** What the setting is called when it stands alone, as in a search
	 *  result. Sentence case. `{name}` is the assistant's name. */
	title: string;
	section: SectionId;
	kind: SettingKind;
	/** The words people type for it, beyond those in the title. */
	words: string[];
	/** Where it lives: `/virtues` for a row on the Settings page, or the
	 *  page a row opens. */
	route: string;
	/** Undoing it is not possible (unpair everything, start over). */
	destructive?: boolean;
}

export const SECTIONS: readonly Section[] = [
	{ id: 'you', title: 'You', lede: 'The basics {name} and your pages work from.' },
	{ id: 'assistant', title: '{name}', lede: 'How it talks, what it remembers, and the models it runs on.' },
	{ id: 'data', title: 'Your data', lede: "The keys your server keeps, and what you've shared." },
	{ id: 'appearance', title: 'Appearance', lede: 'How the app looks.' },
	{ id: 'billing', title: 'Billing', lede: 'Your subscription, what AI has cost, and what it may spend.' },
	{ id: 'server', title: 'Server and devices', lede: 'Updates, reaching it from anywhere, and everything paired with it.' },
];

export const SETTINGS: readonly Setting[] = [
	// You
	{ id: 'full-name', title: 'Your name', section: 'you', kind: 'control', words: ['full name', 'profile', 'me'], route: '/virtues' },
	{ id: 'preferred-name', title: 'What {name} calls you', section: 'you', kind: 'control', words: ['preferred name', 'nickname', 'call me'], route: '/virtues' },
	{ id: 'birthday', title: 'Birthday', section: 'you', kind: 'control', words: ['birth date', 'age', 'born'], route: '/virtues' },
	{ id: 'occupation', title: 'Work', section: 'you', kind: 'control', words: ['occupation', 'job', 'role'], route: '/virtues' },
	{ id: 'employer', title: 'Where you work', section: 'you', kind: 'control', words: ['employer', 'company', 'job'], route: '/virtues' },
	{ id: 'home', title: 'Where you live', section: 'you', kind: 'control', words: ['home', 'time zone', 'timezone', 'city', 'location'], route: '/virtues' },

	// The assistant
	{ id: 'assistant-name', title: "{name}'s name", section: 'assistant', kind: 'control', words: ['assistant', 'rename', 'call it'], route: '/virtues' },
	{ id: 'style', title: 'How {name} talks to you', section: 'assistant', kind: 'control', words: ['style', 'tone', 'shorter', 'longer', 'answers', 'voice', 'instructions'], route: '/virtues' },
	{ id: 'subjects', title: 'Subjects {name} never raises', section: 'assistant', kind: 'control', words: ['rules', 'avoid', 'never bring up', 'topics'], route: '/virtues' },
	{ id: 'memory', title: 'What {name} remembers', section: 'assistant', kind: 'page', words: ['memory', 'memories', 'notes', 'forget'], route: '/virtues/assistant/memory' },
	{ id: 'models', title: 'Models {name} runs on', section: 'assistant', kind: 'control', words: ['model', 'ai', 'chat model', 'background', 'faster', 'cheaper', 'smarter'], route: '/virtues' },
	{ id: 'model-catalog', title: 'Every model you can pick', section: 'assistant', kind: 'page', words: ['models', 'catalog', 'compare'], route: '/virtues/assistant/models' },
	{ id: 'suggested-questions', title: 'Suggested questions', section: 'assistant', kind: 'control', words: ['suggestions', 'prompts', 'new chat', 'ideas'], route: '/virtues' },

	// Your data
	{ id: 'keys', title: 'Keys', section: 'data', kind: 'page', words: ['api key', 'secrets', 'tokens', 'credentials'], route: '/virtues/keys' },
	{ id: 'shared-links', title: 'Shared links', section: 'data', kind: 'page', words: ['share', 'public', 'links', 'turn off'], route: '/virtues/shared' },

	// Appearance
	{ id: 'theme', title: 'Theme', section: 'appearance', kind: 'control', words: ['dark', 'light', 'mode', 'color', 'colour', 'appearance', 'look'], route: '/virtues' },

	// Billing
	{ id: 'subscription', title: 'Subscription', section: 'billing', kind: 'action', words: ['plan', 'stripe', 'payment', 'card', 'cancel', 'renew'], route: '/virtues' },
	{ id: 'balance', title: 'Balance', section: 'billing', kind: 'value', words: ['wallet', 'credit', 'money', 'left'], route: '/virtues' },
	{ id: 'auto-top-up', title: 'Auto top-up', section: 'billing', kind: 'control', words: ['top up', 'refill', 'add money', 'limit'], route: '/virtues' },
	{ id: 'limit-day', title: 'Most a request can spend in a day', section: 'billing', kind: 'control', words: ['spending limit', 'daily limit', 'cap', 'budget', 'spend'], route: '/virtues' },
	{ id: 'limit-run', title: 'Most one request can spend', section: 'billing', kind: 'control', words: ['spending limit', 'per run', 'cap', 'budget', 'spend'], route: '/virtues' },
	{ id: 'byo-ai', title: 'Bring your own AI', section: 'billing', kind: 'page', words: ['own key', 'provider', 'endpoint', 'openai', 'byo'], route: '/virtues/billing/byo' },
	{ id: 'ai-cost', title: 'What AI has cost', section: 'billing', kind: 'page', words: ['usage', 'spend', 'cost', 'calls', 'spending'], route: '/virtues/billing' },

	// Server and devices
	{ id: 'updates', title: 'Check for updates', section: 'server', kind: 'action', words: ['update', 'version', 'upgrade', 'release'], route: '/virtues' },
	{ id: 'auto-install', title: 'Install updates on their own', section: 'server', kind: 'control', words: ['automatic updates', 'auto update'], route: '/virtues' },
	{ id: 'update-channel', title: 'Which updates', section: 'server', kind: 'control', words: ['channel', 'early', 'beta', 'released', 'prerelease'], route: '/virtues' },
	{ id: 'reach', title: 'Reach your server away from home', section: 'server', kind: 'control', words: ['remote', 'relay', 'network', 'anywhere', 'away'], route: '/virtues' },
	{ id: 'search-runs-on', title: 'Where search runs', section: 'server', kind: 'page', words: ['search', 'accelerator', 'npu', 'cpu', 'index'], route: '/virtues/search' },
	{ id: 'server-health', title: 'How your server is doing', section: 'server', kind: 'page', words: ['system', 'storage', 'disk', 'memory', 'cpu', 'health', 'backup'], route: '/virtues/system' },
	{ id: 'devices', title: 'Devices', section: 'server', kind: 'page', words: ['phone', 'iphone', 'mac', 'paired', 'unpair'], route: '/virtues/devices' },
	{ id: 'pair-device', title: 'Pair a device', section: 'server', kind: 'action', words: ['add device', 'connect', 'new phone'], route: '/virtues/devices' },
	{ id: 'start-over', title: 'Start over', section: 'server', kind: 'action', words: ['reset', 'unpair everything', 'setup again'], route: '/virtues', destructive: true },
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
