// @vitest-environment happy-dom
/**
 * Where a link to the web opens in the app on a computer (`webLinks.ts`): the
 * Browser tab on a click, the default browser on ⌘-click, a middle click, the
 * menu, or while the assistant is using the Browser. Links on the box and
 * links in an editor are left to their own handlers.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('$lib/utils/platform', () => ({ isTauri: true, isIOS: false }));
vi.mock('svelte-sonner', () => ({ toast: { info: vi.fn() } }));
vi.mock('$lib/stores/contextMenu.svelte', () => ({ contextMenu: { show: vi.fn() } }));
vi.mock('$lib/stores/browserAgent.svelte', () => ({ browserAgent: { driving: false } }));
vi.mock('$lib/components/tabs/browserPaneListener', () => ({ showBrowserTab: vi.fn() }));
vi.mock('$lib/tauri/bridge', () => ({
	browserPaneOpen: vi.fn(async () => {}),
	canBrowserPane: vi.fn(async () => true),
	openExternal: vi.fn(async () => {}),
}));

import { toast } from 'svelte-sonner';
import { contextMenu } from '$lib/stores/contextMenu.svelte';
import { browserAgent } from '$lib/stores/browserAgent.svelte';
import { showBrowserTab } from '$lib/components/tabs/browserPaneListener';
import { browserPaneOpen, canBrowserPane, openExternal } from '$lib/tauri/bridge';
import { setBackendOrigin } from '$lib/config/backend';
import { ASSISTANT_BROWSING_NOTICE, isWebLink, listenForWebLinks, openWebLink } from './webLinks';

const PAGE = 'https://example.com/article';
const BOX = 'http://127.0.0.1:7117';

beforeEach(() => {
	vi.clearAllMocks();
	browserAgent.driving = false;
	vi.mocked(canBrowserPane).mockResolvedValue(true);
	vi.mocked(browserPaneOpen).mockResolvedValue(undefined);
	setBackendOrigin(BOX);
});

describe('isWebLink', () => {
	it('is an http or https address off the box', () => {
		expect(isWebLink(PAGE)).toBe(true);
		expect(isWebLink('http://example.com')).toBe(true);
	});

	it('is not the box, by its address or by this page', () => {
		expect(isWebLink(`${BOX}/api/drive/files/f1/download`)).toBe(false);
		expect(isWebLink(`${location.origin}/chat`)).toBe(false);
	});

	it('is not another scheme or a route', () => {
		for (const href of ['mailto:nick@example.com', 'javascript:alert(1)', '/person/p1', 'not a url']) {
			expect(isWebLink(href)).toBe(false);
		}
	});
});

describe('openWebLink', () => {
	it('opens the page in the Browser tab on the Mac', async () => {
		await openWebLink(PAGE);
		expect(browserPaneOpen).toHaveBeenCalledWith(PAGE);
		expect(showBrowserTab).toHaveBeenCalledWith(PAGE);
		expect(openExternal).not.toHaveBeenCalled();
	});

	it('leaves the Browser to the assistant while it is using it, and says so', async () => {
		browserAgent.driving = true;
		await openWebLink(PAGE);
		expect(browserPaneOpen).not.toHaveBeenCalled();
		expect(openExternal).toHaveBeenCalledWith(PAGE);
		expect(toast.info).toHaveBeenCalledWith(ASSISTANT_BROWSING_NOTICE);
	});

	it('opens the default browser where there is no Browser tab', async () => {
		vi.mocked(canBrowserPane).mockResolvedValue(false);
		await openWebLink(PAGE);
		expect(openExternal).toHaveBeenCalledWith(PAGE);
		expect(showBrowserTab).not.toHaveBeenCalled();
	});

	it('opens the default browser when the Browser refuses the page', async () => {
		vi.mocked(browserPaneOpen).mockRejectedValue(new Error('Only http and https pages can be opened.'));
		vi.spyOn(console, 'error').mockImplementation(() => {});
		await openWebLink(PAGE);
		expect(openExternal).toHaveBeenCalledWith(PAGE);
		expect(showBrowserTab).not.toHaveBeenCalled();
	});

	it('hands an email address to its own app', async () => {
		await openWebLink('mailto:nick@example.com');
		expect(browserPaneOpen).not.toHaveBeenCalled();
		expect(openExternal).toHaveBeenCalledWith('mailto:nick@example.com');
	});
});

describe('listenForWebLinks', () => {
	let stop: () => void;
	/** Whether the listener took the last event; then stops happy-dom following the link. */
	let taken: boolean | null;
	const record = (e: Event) => {
		taken = e.defaultPrevented;
		e.preventDefault();
	};

	beforeEach(() => {
		taken = null;
		stop = listenForWebLinks();
		for (const type of ['click', 'auxclick', 'contextmenu']) window.addEventListener(type, record);
	});

	afterEach(() => {
		stop();
		for (const type of ['click', 'auxclick', 'contextmenu']) window.removeEventListener(type, record);
		document.body.innerHTML = '';
	});

	function link(href: string, inside?: string): HTMLAnchorElement {
		const a = document.createElement('a');
		a.href = href;
		a.target = '_blank';
		a.textContent = 'a link';
		if (inside) {
			const host = document.createElement('div');
			host.className = inside;
			host.append(a);
			document.body.append(host);
		} else {
			document.body.append(a);
		}
		return a;
	}

	function press(a: Element, type: string, init: MouseEventInit = {}) {
		a.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, ...init }));
	}

	it('opens a clicked link in the Browser tab', async () => {
		press(link(PAGE), 'click');
		expect(taken).toBe(true);
		await vi.waitFor(() => expect(browserPaneOpen).toHaveBeenCalledWith(PAGE));
		expect(openExternal).not.toHaveBeenCalled();
	});

	it('opens a ⌘-clicked or middle-clicked link in the default browser', () => {
		press(link(PAGE), 'click', { metaKey: true });
		expect(openExternal).toHaveBeenLastCalledWith(PAGE);
		press(link(PAGE), 'auxclick', { button: 1 });
		expect(openExternal).toHaveBeenCalledTimes(2);
		expect(browserPaneOpen).not.toHaveBeenCalled();
	});

	it('gives a right-clicked link its menu', () => {
		press(link(PAGE), 'contextmenu', { clientX: 10, clientY: 20 });
		expect(taken).toBe(true);
		const [at, items] = vi.mocked(contextMenu.show).mock.calls[0];
		expect(at).toEqual({ x: 10, y: 20 });
		expect(items.map((i) => i.id)).toEqual(['open', 'open-default-browser', 'copy-link']);
		items[1].action?.();
		expect(openExternal).toHaveBeenCalledWith(PAGE);
	});

	it('leaves a link in an editor to the editor', () => {
		press(link(PAGE, 'cm-editor'), 'click');
		press(link(PAGE, 'ProseMirror'), 'contextmenu');
		expect(taken).toBe(false);
		expect(openExternal).not.toHaveBeenCalled();
		expect(contextMenu.show).not.toHaveBeenCalled();
	});

	it('leaves a link a view already handled to that view', () => {
		const a = link(PAGE);
		a.addEventListener('click', (e) => e.preventDefault());
		press(a, 'click');
		expect(browserPaneOpen).not.toHaveBeenCalled();
		expect(openExternal).not.toHaveBeenCalled();
	});

	it('leaves a link on the box alone', () => {
		press(link(`${BOX}/api/drive/files/f1/download`), 'click');
		expect(taken).toBe(false);
		expect(openExternal).not.toHaveBeenCalled();
	});
});
