/**
 * The link panel's first focus: a new link's address field, so a URL pasted
 * after ⌘K lands there and not over the words being linked; an existing
 * link's label, selected, for a correction typed over it.
 */

import { afterEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';
import LinkEditorPopover from './LinkEditorPopover.svelte';
import { linkEditor } from '$lib/stores/linkEditor.svelte';

let open: ReturnType<typeof mount> | null = null;

afterEach(() => {
	if (open) unmount(open);
	open = null;
	linkEditor.hide();
	document.body.replaceChildren();
});

function show(value: { label: string; href: string }): HTMLInputElement[] {
	linkEditor.show(value, () => {});
	open = mount(LinkEditorPopover, { target: document.body });
	flushSync();
	return [...document.body.querySelectorAll('input')];
}

describe('the link panel', () => {
	it('focuses the address of a new link, and keeps it there as one is typed', () => {
		const [label, href] = show({ label: 'the notes', href: '' });
		expect(document.activeElement).toBe(href);
		href.value = 'https://example.com/notes';
		href.dispatchEvent(new Event('input', { bubbles: true }));
		flushSync();
		expect(document.activeElement).toBe(href);
		expect(label.value).toBe('the notes');
	});

	it('selects the label of a link that has an address', () => {
		const [label] = show({ label: 'the notes', href: 'https://example.com/notes' });
		expect(document.activeElement).toBe(label);
		expect([label.selectionStart, label.selectionEnd]).toEqual([0, 'the notes'.length]);
	});
});
