/**
 * Entity Badge Utilities
 *
 * Single source of truth for entity badge rendering across:
 * - EntityChip.svelte (Svelte context)
 * - ChatInput.svelte (contenteditable)
 * - ProseMirror EntityLinkView (NodeView)
 */

/**
 * Create entity badge HTML for contenteditable contexts
 * Returns a span element with @name, styled consistently
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

	// Non-editable in contenteditable contexts
	element.contentEditable = 'false';

	// Simple @name format
	element.textContent = `@${displayName}`;

	return element;
}
