import { describe, expect, it } from 'vitest';
import { crumbsForRoute } from './crumbs';

const labels = (route: string, title: string) => crumbsForRoute(route, title).crumbs.map((c) => c.label);

describe("Settings' crumbs", () => {
	it('put each page under its heading in the landing page site map', () => {
		expect(labels('/virtues/assistant', 'Settings · How Ari talks to you')).toEqual(['Settings', 'Your assistant', 'How it talks']);
		expect(labels('/virtues/assistant/tools', 'Settings · What Ari can do')).toEqual(['Settings', 'Your assistant', 'What it can do']);
		expect(labels('/virtues/assistant/spending', 'Settings · Models and spending')).toEqual(['Settings', 'Your assistant', 'Models and spending']);
		expect(labels('/virtues/data', 'Settings · What Ari may read')).toEqual(['Settings', 'Your server', 'Your data']);
		expect(labels('/virtues/billing', 'Settings · Billing')).toEqual(['Settings', 'Your profile', 'Billing']);
		expect(labels('/virtues/data/keys', 'Settings · Keys')).toEqual(['Settings', 'Your server', 'Your data', 'Keys']);
		expect(labels('/virtues/data/shared', 'Settings · Shared links')).toEqual(['Settings', 'Your server', 'Your data', 'Shared links']);
	});

	it('keep a page under its parent page', () => {
		expect(labels('/virtues/assistant/spending/cost', 'Settings · What AI has cost')).toEqual([
			'Settings',
			'Your assistant',
			'Models and spending',
			'What AI has cost',
		]);
		expect(labels('/virtues/assistant/spending/models', 'Settings · Every model you can pick')).toEqual([
			'Settings',
			'Your assistant',
			'Models and spending',
			'Every model you can pick',
		]);
	});
});
