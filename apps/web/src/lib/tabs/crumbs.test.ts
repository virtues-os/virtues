import { describe, expect, it } from 'vitest';
import { crumbsForRoute } from './crumbs';

const labels = (route: string, title: string) => crumbsForRoute(route, title).crumbs.map((c) => c.label);

describe("Settings' crumbs", () => {
	it('put each page under its heading in the landing page site map', () => {
		expect(labels('/virtues/assistant/memory', 'Settings · What Ari remembers')).toEqual(['Settings', 'Assistant', 'Memory']);
		expect(labels('/virtues/data', 'Settings · What Ari may read')).toEqual(['Settings', 'Assistant', 'Data access']);
		expect(labels('/virtues/billing', 'Settings · Billing')).toEqual(['Settings', 'You', 'Billing']);
		expect(labels('/virtues/keys', 'Settings · Keys')).toEqual(['Settings', 'Your server', 'Keys']);
	});

	it("name the assistant's page once, as its heading", () => {
		expect(labels('/virtues/assistant', 'Settings · Ari')).toEqual(['Settings', 'Assistant']);
		expect(labels('/virtues/assistant/models', 'Settings · Every model you can pick')).toEqual(['Settings', 'Assistant', 'Every model you can pick']);
	});

	it('keep a page under its parent page', () => {
		expect(labels('/virtues/billing/cost', 'Settings · What AI has cost')).toEqual(['Settings', 'You', 'Billing', 'What AI has cost']);
	});
});
