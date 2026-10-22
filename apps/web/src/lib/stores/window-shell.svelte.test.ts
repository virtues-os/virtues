import { describe, it, expect, vi, beforeEach } from 'vitest';

// The real registry loads every view (the chat among them); the shell only
// needs a route's type and label from it.
vi.mock('$lib/tabs/registry', () => ({
	tabRegistry: { chat: {}, page: {} },
	parseRoute: (route: string) => ({
		type: route.startsWith('/page') ? 'page' : 'chat',
		label: route,
		normalizedRoute: route,
		entityId: route.split('/')[2],
	}),
}));

import { windowShellStore as shell } from './window-shell.svelte';

/** The routes of a pane's tabs, in strip order. */
function strip(paneId = 'left'): string[] {
	const pane = shell.panes.find((p) => p.id === paneId);
	return (pane?.tabs ?? []).map((t) => t.route);
}

function active(): string | undefined {
	return shell.activeTab?.route;
}

beforeEach(() => {
	localStorage.clear();
	shell.panes = [{ id: 'left', tabs: [], activeTabId: null, width: 100 }];
	shell.activePaneId = 'left';
	shell.openTabFromRoute('/chat/chat_a', { forceNew: true });
});

describe('opening things', () => {
	it('opens a link in the tab you are on, as a browser does', () => {
		shell.openTabFromRoute('/page/page_1');
		expect(strip()).toEqual(['/page/page_1']);
		shell.openTabFromRoute('/page/page_2');
		expect(strip()).toEqual(['/page/page_2']);
	});

	it('walks the tab in place, so Back returns', () => {
		shell.openTabFromRoute('/page/page_1');
		shell.goBack();
		expect(active()).toBe('/chat/chat_a');
		expect(strip()).toEqual(['/chat/chat_a']);
	});

	it('opens a new tab next to the current one when asked (a ⌘-click)', () => {
		shell.openTabFromRoute('/page/page_1', { forceNew: true });
		const chat = shell.panes[0].tabs[0];
		shell.setActiveTab(chat.id);
		shell.openTabFromRoute('/page/page_2', { forceNew: true, adjacent: true });
		expect(strip()).toEqual(['/chat/chat_a', '/page/page_2', '/page/page_1']);
		expect(active()).toBe('/page/page_2');
	});

	it('switches to a tab that already shows the thing', () => {
		shell.openTabFromRoute('/page/page_1', { forceNew: true });
		shell.setActiveTab(shell.panes[0].tabs[0].id);
		shell.openTabFromRoute('/page/page_1', { focusExisting: true });
		expect(strip()).toEqual(['/chat/chat_a', '/page/page_1']);
		expect(active()).toBe('/page/page_1');
	});

	it('starts a new chat in a new tab, and reuses an empty one', () => {
		shell.newChat();
		expect(strip()).toEqual(['/chat/chat_a', '/']);
		shell.setActiveTab(shell.panes[0].tabs[0].id);
		shell.newChat();
		expect(strip()).toEqual(['/chat/chat_a', '/']);
		expect(active()).toBe('/');
	});

	it('moves within a view in place', () => {
		shell.navigate('/chat/chat_c');
		expect(strip()).toEqual(['/chat/chat_c']);
	});

	it('opens beside as a new tab in the other pane, without replacing it', () => {
		shell.openRouteBeside('/page/page_1');
		// The second reference is clicked in the left pane, which focuses it.
		shell.setActivePane('left');
		shell.openRouteBeside('/page/page_2');
		expect(strip('right')).toEqual(['/page/page_1', '/page/page_2']);
		shell.setActivePane('left');
		shell.openRouteBeside('/page/page_1');
		expect(strip('right')).toEqual(['/page/page_1', '/page/page_2']);
	});

	it('closes the tabs to the left', () => {
		shell.openTabFromRoute('/page/page_1', { forceNew: true });
		shell.openTabFromRoute('/page/page_2', { forceNew: true });
		shell.closeTabsToLeft(shell.activeTab!.id);
		expect(strip()).toEqual(['/page/page_2']);
	});
});
