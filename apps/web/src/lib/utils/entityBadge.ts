/**
 * Entity Badge Utilities
 *
 * Single source of truth for entity badge rendering across:
 * - ChatInput.svelte (contenteditable composer chips)
 * - CodeMirror EntityLinkWidget (decoration)
 *
 * (EntityChip.svelte renders the Svelte equivalent for assistant messages.)
 */

import { getEntityTypeFromRoute } from './entityRoutes';
import riUserLine from '@iconify-icons/ri/user-line';
import riMapPinLine from '@iconify-icons/ri/map-pin-line';
import riBuildingLine from '@iconify-icons/ri/building-line';
import riLightbulbLine from '@iconify-icons/ri/lightbulb-line';
import riFileTextLine from '@iconify-icons/ri/file-text-line';
import riChat3Line from '@iconify-icons/ri/chat-3-line';
import riFolderLine from '@iconify-icons/ri/folder-line';
import riFileLine from '@iconify-icons/ri/file-line';
import riAtLine from '@iconify-icons/ri/at-line';

type IconData = { body: string; width?: number; height?: number };

const TYPE_ICON_DATA: Record<string, IconData> = {
	person: riUserLine,
	place: riMapPinLine,
	org: riBuildingLine,
	thing: riLightbulbLine,
	page: riFileTextLine,
	chat: riChat3Line,
	space: riFolderLine,
	file: riFileLine,
};

/** Build a trusted inline SVG (icon data is bundled, not user input). */
function typeIconSvg(type: string | null): string {
	const data = (type && TYPE_ICON_DATA[type]) || (riAtLine as IconData);
	const w = data.width ?? 24;
	const h = data.height ?? 24;
	return `<svg class="entity-chip-icon" viewBox="0 0 ${w} ${h}" width="11" height="11" fill="currentColor" aria-hidden="true">${data.body}</svg>`;
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
	const { tagName = 'span', className = 'entity-badge' } = options;

	const element = document.createElement(tagName);
	element.className = className;
	element.setAttribute('data-entity-url', entityUrl);

	if (tagName === 'a') {
		(element as HTMLAnchorElement).href = entityUrl;
	}

	// Non-editable in contenteditable contexts (atomic chip).
	element.contentEditable = 'false';

	// Leading type icon (trusted SVG) + @name (textContent → no XSS).
	const type = getEntityTypeFromRoute(entityUrl);
	element.innerHTML = typeIconSvg(type);
	const label = document.createElement('span');
	label.textContent = `@${displayName}`;
	element.appendChild(label);

	return element;
}
