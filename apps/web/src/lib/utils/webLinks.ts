/**
 * Where a link to the web opens.
 *
 * The app's webview drops `target="_blank"` and `window.open` to another
 * site, and refuses to leave its own page (`is_own_page` in
 * src-tauri/src/main.rs), so a link left to the webview goes nowhere. In the
 * app on a computer every link to the web opens through here:
 *
 * - A click opens the page in the Browser tab beside the current view, on
 *   the Mac (`canBrowserPane`). Elsewhere it opens in the computer's default
 *   browser. So does a click while the assistant is using the Browser: there
 *   is one Browser, and the click would take the page out from under it.
 * - ⌘ or Ctrl-click, a middle click, and the link menu's "Open in default
 *   browser" open it in the default browser (`openExternal`). The Browser tab
 *   keeps its own sign-ins, so a site you are signed in to elsewhere opens
 *   signed out in it.
 * - `mailto:` and `tel:` open in their own app.
 *
 * An anchor needs nothing of its own: `listenForWebLinks` takes the click,
 * middle click and right-click on any `<a href>` to the web. Editors own
 * their links (`document/links.ts`) and
 * call `openWebLink` themselves. In a browser, and in the phone app, a link
 * opens as it always has.
 *
 * An address on the box is not a link to the web: the box's own files need
 * the box's sign-in, which neither browser has.
 */
import { toast } from 'svelte-sonner';
import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { getBackendOrigin } from '$lib/config/backend';
import { browserPaneOpen, canBrowserPane, openExternal } from '$lib/tauri/bridge';
import { isIOS, isTauri } from '$lib/utils/platform';

/** The app on a computer, the one place a link needs routing. */
export const inDesktopApp = isTauri && !isIOS;

export const ASSISTANT_BROWSING_NOTICE =
	'Your assistant is using the Browser, so the link opened in your default browser.';

function parse(href: string): URL | null {
	try {
		return new URL(href);
	} catch {
		return null;
	}
}

function onTheBox(u: URL): boolean {
	if (typeof location !== 'undefined' && u.host === location.host) return true;
	const box = parse(getBackendOrigin());
	return !!box && u.host === box.host;
}

/** A page on the web: an http or https address that is not the box's. */
export function isWebLink(href: string): boolean {
	const u = parse(href);
	return !!u && (u.protocol === 'http:' || u.protocol === 'https:') && !onTheBox(u);
}

/** An address another app opens: an email, a call. */
function isAppLink(href: string): boolean {
	return /^(mailto|tel):/i.test(href);
}

/** Open a link to the web where a click on it goes (the header says where). */
export async function openWebLink(url: string): Promise<void> {
	if (!inDesktopApp) {
		window.open(url, '_blank', 'noopener,noreferrer');
		return;
	}
	if (isWebLink(url) && (await canBrowserPane())) {
		// Loaded here, not above: the Browser tab's modules carry the tab
		// registry, every view in the app, which the page editor's links
		// (`document/links.ts`) have no use for.
		const [{ browserAgent }, { showBrowserTab }] = await Promise.all([
			import('$lib/stores/browserAgent.svelte'),
			import('$lib/components/tabs/browserPaneListener'),
		]);
		if (browserAgent.driving) {
			toast.info(ASSISTANT_BROWSING_NOTICE);
		} else {
			try {
				await browserPaneOpen(url);
				showBrowserTab(url);
				return;
			} catch (e) {
				console.error('[webLinks] the Browser refused the page; opening it in the default browser', e);
			}
		}
	}
	await openExternal(url);
}

/**
 * "Open in default browser" for a link to the web, in the app on a computer.
 * In a browser, the menu's Open already opens one.
 */
export function defaultBrowserItem(url: string): ContextMenuItem[] {
	if (!inDesktopApp || !isWebLink(url)) return [];
	return [
		{
			id: 'open-default-browser',
			label: 'Open in default browser',
			icon: 'ri:external-link-line',
			action: () => void openExternal(url),
		},
	];
}

/** The menu on a link to the web: Open, Open in default browser, Copy link. */
export function webLinkMenu(url: string): ContextMenuItem[] {
	return [
		{ id: 'open', label: 'Open', icon: 'ri:arrow-right-up-line', action: () => void openWebLink(url) },
		...defaultBrowserItem(url),
		{
			id: 'copy-link',
			label: 'Copy link',
			icon: 'ri:file-copy-line',
			action: () => void navigator.clipboard?.writeText(url),
		},
	];
}

/**
 * The address of the link a pointer event is on, when this module opens it:
 * a link to the web or to another app that nothing else has handled, and
 * that is not in an editor (a click there places the caret).
 */
function linkOf(e: MouseEvent): string | null {
	if (e.defaultPrevented) return null;
	const a = (e.target as Element | null)?.closest?.('a[href]') as HTMLAnchorElement | null;
	if (!a || a.hasAttribute('download')) return null;
	if (a.closest('.ProseMirror, [contenteditable="true"]')) return null;
	const url = a.href;
	return isWebLink(url) || isAppLink(url) ? url : null;
}

/**
 * Open every link to the web in the page as the header says. Listens on the
 * document, so a view's own handler runs first, and one that handled the
 * event (`preventDefault`) keeps it.
 */
export function listenForWebLinks(): () => void {
	if (!inDesktopApp) return () => {};
	const onClick = (e: MouseEvent) => {
		if (e.button !== 0) return;
		const url = linkOf(e);
		if (!url) return;
		e.preventDefault();
		if (e.metaKey || e.ctrlKey || e.shiftKey) void openExternal(url);
		else void openWebLink(url);
	};
	const onAuxClick = (e: MouseEvent) => {
		if (e.button !== 1) return;
		const url = linkOf(e);
		if (!url) return;
		e.preventDefault();
		void openExternal(url);
	};
	const onContextMenu = (e: MouseEvent) => {
		const url = linkOf(e);
		if (!url) return;
		e.preventDefault();
		contextMenu.show({ x: e.clientX, y: e.clientY }, webLinkMenu(url));
	};
	document.addEventListener('click', onClick);
	document.addEventListener('auxclick', onAuxClick);
	document.addEventListener('contextmenu', onContextMenu);
	return () => {
		document.removeEventListener('click', onClick);
		document.removeEventListener('auxclick', onAuxClick);
		document.removeEventListener('contextmenu', onContextMenu);
	};
}
