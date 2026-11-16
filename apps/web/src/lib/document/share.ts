/**
 * The address to copy for a link: a ref as a full link to this box.
 *
 * Apart from `links.ts` because the chat composer's mention chip needs it, and
 * `links.ts` pulls in the Yjs anchors only a page editor uses.
 */
export function shareableHref(href: string): string {
	return href.startsWith('/') ? `${window.location.origin}${href}` : href;
}
