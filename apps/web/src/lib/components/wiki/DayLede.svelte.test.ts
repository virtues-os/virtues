/**
 * A stored lede carries markdown links and the veil's ⟦ ⟧ marks, sometimes a
 * mark inside a link's label. A list that printed it as text showed both as
 * raw markup; these check that the names draw as links and no mark is left.
 */

import { afterEach, describe, expect, it } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

const { default: DayLede } = await import('./DayLede.svelte');

let mounted: ReturnType<typeof mount> | null = null;

function draw(markdown: string): HTMLElement {
	const target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(DayLede, { target, props: { markdown } });
	flushSync();
	return target;
}

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	document.body.innerHTML = '';
});

describe('DayLede', () => {
	it('draws a marked name inside a link as a link, with no markup left', () => {
		const el = draw('Dinner with [⟦Nick⟧](/person/person_0001) and ⟦David Okafor⟧ at home.');
		const text = el.textContent ?? '';
		expect(text).toContain('Dinner with Nick and David Okafor at home.');
		expect(text).not.toMatch(/[⟦⟧[\]()]/);
		expect(el.querySelector('a')?.getAttribute('href')).toContain('/person/person_0001');
	});

	it('reads plain prose as it is', () => {
		expect(draw('A quiet day.').textContent).toBe('A quiet day.');
	});
});
