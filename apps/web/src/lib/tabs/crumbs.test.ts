import { describe, expect, it } from 'vitest';
import { crumbsForRoute } from './crumbs';

const labels = (route: string, title: string) => crumbsForRoute(route, title).crumbs.map((c) => c.label);

describe("Settings' crumbs", () => {
	it('put each page under its heading in the landing page site map', () => {
		expect(labels('/virtues/assistant', 'Settings · How Ari talks to you')).toEqual(['Settings', 'Assistant', 'Name and style']);
		expect(labels('/virtues/assistant/tools', 'Settings · What Ari can do')).toEqual(['Settings', 'Assistant', 'Tools']);
		expect(labels('/virtues/assistant/spending', 'Settings · Models and spending')).toEqual(['Settings', 'Assistant', 'Models']);
		expect(labels('/virtues/data', 'Settings · What Ari may read')).toEqual(['Settings', 'Server', 'Data']);
		expect(labels('/virtues/billing', 'Settings · Billing')).toEqual(['Settings', 'Account', 'Billing']);
		expect(labels('/virtues/data/keys', 'Settings · Keys')).toEqual(['Settings', 'Server', 'Data', 'Keys']);
		expect(labels('/virtues/data/shared', 'Settings · Shared links')).toEqual(['Settings', 'Server', 'Data', 'Shared links']);
	});

	it("put Server's pages under its name, and This device on its own", () => {
		expect(labels('/virtues/system', 'Settings · Your server')).toEqual(['Settings', 'Server', 'Status']);
		expect(labels('/virtues/devices', 'Settings · Devices')).toEqual(['Settings', 'Server', 'Devices']);
		expect(labels('/virtues/updates', 'Settings · Updates')).toEqual(['Settings', 'Server', 'Updates']);
		expect(labels('/virtues/devices/dev_x', 'Settings · Office iMac')).toEqual(['Settings', 'Server', 'Devices', 'Office iMac']);
		expect(labels('/virtues/devices/this', 'Settings · This device')).toEqual(['Settings', 'This device']);
		expect(labels('/sources', 'Sources')).toEqual(['Settings', 'Server', 'Sources']);
	});

	it("link Server's crumb to the landing page, where its links are", () => {
		const crumbs = crumbsForRoute('/virtues/devices', 'Settings · Devices').crumbs;
		expect(crumbs[1]).toMatchObject({ label: 'Server', route: '/virtues' });
	});

	it('keep a page under its parent page', () => {
		expect(labels('/virtues/assistant/spending/cost', 'Settings · What AI has cost')).toEqual([
			'Settings',
			'Assistant',
			'Usage',
		]);
		expect(labels('/virtues/assistant/spending/models', 'Settings · Every model you can pick')).toEqual([
			'Settings',
			'Assistant',
			'Models',
			'Every model you can pick',
		]);
	});
});
