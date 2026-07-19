/**
 * Entity Badge Utilities
 *
 * Single source of truth for entity badge rendering across:
 * - ChatInput.svelte (contenteditable composer chips)
 * - CodeMirror EntityLinkWidget (decoration)
 *
 * (Ref.svelte renders the Svelte equivalent for assistant messages.)
 */

import { getEntityTypeFromRoute, refIcon } from './refRoutes';
import riUserLine from '@iconify-icons/ri/user-line';
import riMapPinLine from '@iconify-icons/ri/map-pin-line';
import riBuildingLine from '@iconify-icons/ri/building-line';
import riLightbulbLine from '@iconify-icons/ri/lightbulb-line';
import riFileTextLine from '@iconify-icons/ri/file-text-line';
import riChat3Line from '@iconify-icons/ri/chat-3-line';
import riFolderLine from '@iconify-icons/ri/folder-line';
import riFileLine from '@iconify-icons/ri/file-line';
import riFilePdfLine from '@iconify-icons/ri/file-pdf-line';
import riImageLine from '@iconify-icons/ri/image-line';
import riMusic2Line from '@iconify-icons/ri/music-2-line';
import riMovieLine from '@iconify-icons/ri/movie-line';
import riCalendarLine from '@iconify-icons/ri/calendar-line';
import riAtLine from '@iconify-icons/ri/at-line';

type IconData = { body: string; width?: number; height?: number };

// Keyed by the iconify names refIcon() can return, so the composer's inline-SVG
// chips render the exact icon every other ref surface resolves to.
const ICON_DATA: Record<string, IconData> = {
	'ri:user-line': riUserLine,
	'ri:map-pin-line': riMapPinLine,
	'ri:building-line': riBuildingLine,
	'ri:lightbulb-line': riLightbulbLine,
	'ri:file-text-line': riFileTextLine,
	'ri:chat-3-line': riChat3Line,
	'ri:folder-line': riFolderLine,
	'ri:file-line': riFileLine,
	'ri:file-pdf-line': riFilePdfLine,
	'ri:image-line': riImageLine,
	'ri:music-2-line': riMusic2Line,
	'ri:movie-line': riMovieLine,
	'ri:calendar-line': riCalendarLine,
	'ri:at-line': riAtLine,
};

/** Build a trusted inline SVG for a resolved ref icon (bundled data, not user
 * input). Inline SVG — not the `<iconify-icon>` web component — so it renders
 * offline / on a self-hosted box with no Iconify network fetch. */
export function refIconSvg(type: string | null, filename?: string, size = 11): string {
	const name = refIcon(type, { filename });
	const data = ICON_DATA[name] ?? (riAtLine as IconData);
	const w = data.width ?? 24;
	const h = data.height ?? 24;
	return `<svg class="ref-pill-icon" viewBox="0 0 ${w} ${h}" width="${size}" height="${size}" fill="currentColor" aria-hidden="true">${data.body}</svg>`;
}

/**
 * Create entity badge HTML for contenteditable contexts.
 * Returns a span/anchor with a type icon + @name, styled consistently and atomic.
 */
export function createEntityBadgeElement(
	displayName: string,
	entityUrl: string,
	options: {
		tagName?: 'span' | 'a' | 'button';
		className?: string;
	} = {}
): HTMLElement {
	const { tagName = 'span', className = 'ref-pill' } = options;

	const element = document.createElement(tagName);
	element.className = className;
	element.setAttribute('data-entity-url', entityUrl);

	if (tagName === 'a') {
		(element as HTMLAnchorElement).href = entityUrl;
	}

	// Non-editable in contenteditable contexts (atomic chip).
	element.contentEditable = 'false';

	// Leading type icon (trusted SVG) + @name (textContent → no XSS).
	// displayName sharpens file icons by extension (photo.png → image icon).
	const type = getEntityTypeFromRoute(entityUrl);
	element.innerHTML = refIconSvg(type, displayName);
	const label = document.createElement('span');
	label.textContent = `@${displayName}`;
	element.appendChild(label);

	return element;
}
