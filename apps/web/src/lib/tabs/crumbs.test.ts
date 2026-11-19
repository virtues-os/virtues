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

	it("put Your server's pages under its page, and This device on its own", () => {
		expect(labels('/virtues/system', 'Settings · Your server')).toEqual(['Settings', 'Your server']);
		expect(labels('/virtues/devices', 'Settings · Devices')).toEqual(['Settings', 'Your server', 'Devices']);
		expect(labels('/virtues/updates', 'Settings · Updates')).toEqual(['Settings', 'Your server', 'Updates']);
		expect(labels('/virtues/devices/dev_x', 'Settings · Office iMac')).toEqual(['Settings', 'Your server', 'Devices', 'Office iMac']);
		expect(labels('/virtues/devices/this', 'Settings · This device')).toEqual(['Settings', 'This device']);
	});

	it("link Your server's crumb to its page", () => {
		const crumbs = crumbsForRoute('/virtues/devices', 'Settings · Devices').crumbs;
		expect(crumbs[1]).toMatchObject({ label: 'Your server', route: '/virtues/system' });
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
