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

/** The routes of a pane's tabs, with the preview marked `~`. */
function strip(paneId = 'left'): string[] {
	const pane = shell.panes.find((p) => p.id === paneId);
	return (pane?.tabs ?? []).map((t) => (t.preview ? `~${t.route}` : t.route));
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

describe('preview tabs', () => {
	it('opens from a kept tab into one preview tab beside it, and reuses it', () => {
		shell.openTabFromRoute('/page/page_1');
		expect(strip()).toEqual(['/chat/chat_a', '~/page/page_1']);
		shell.openTabFromRoute('/page/page_2');
		expect(strip()).toEqual(['/chat/chat_a', '~/page/page_2']);
		expect(active()).toBe('/page/page_2');
	});

	it('walks the preview tab in place, so Back returns', () => {
		shell.openTabFromRoute('/page/page_1');
		shell.openTabFromRoute('/page/page_2');
		shell.goBack();
		expect(active()).toBe('/page/page_1');
		expect(strip()).toEqual(['/chat/chat_a', '~/page/page_1']);
	});

	it('leaves a kept tab alone when you open from it while the preview is behind', () => {
		shell.openTabFromRoute('/page/page_1');
		const chat = shell.panes[0].tabs[0];
		shell.setActiveTab(chat.id);
		shell.openTabFromRoute('/page/page_2');
		expect(strip()).toEqual(['/chat/chat_a', '~/page/page_2']);
		expect(shell.panes[0].tabs[0].route).toBe('/chat/chat_a');
	});

	it('keeps a tab when asked, and the next open makes a new preview', () => {
		shell.openTabFromRoute('/page/page_1');
		shell.keepTab(shell.activeTab!.id);
		shell.openTabFromRoute('/page/page_2');
		expect(strip()).toEqual(['/chat/chat_a', '/page/page_1', '~/page/page_2']);
	});

	it('keeps a new chat once it gets its id from the first message', () => {
		const id = shell.newChat();
		expect(strip()).toEqual(['/chat/chat_a', '~/']);
		shell.updateTab(id, { route: '/chat/chat_b' });
		expect(strip()).toEqual(['/chat/chat_a', '/chat/chat_b']);
	});

	it('keeps the preview you were looking at when you start a new chat', () => {
		shell.openTabFromRoute('/page/page_1');
		shell.newChat();
		expect(strip()).toEqual(['/chat/chat_a', '/page/page_1', '~/']);
	});

	it('moves within a view in place, even in a kept tab', () => {
		shell.navigate('/chat/chat_c');
		expect(strip()).toEqual(['/chat/chat_c']);
	});

	it('opens beside into the other pane’s preview tab', () => {
		shell.openRouteBeside('/page/page_1');
		// The second reference is clicked in the left pane, which focuses it.
		shell.setActivePane('left');
		shell.openRouteBeside('/page/page_2');
		expect(strip('right')).toEqual(['~/page/page_2']);
	});

	it('closes the tabs to the left', () => {
		shell.openTabFromRoute('/page/page_1', { forceNew: true });
		shell.openTabFromRoute('/page/page_2', { forceNew: true });
		shell.closeTabsToLeft(shell.activeTab!.id);
		expect(strip()).toEqual(['/page/page_2']);
	});
});
