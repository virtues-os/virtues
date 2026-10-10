// @vitest-environment happy-dom
/**
 * A press in a dialog opened over a popover does not dismiss the popover: the
 * version panel's restore confirm, pressed, left nothing to show its outcome.
 */

import { afterEach, describe, expect, it } from 'vitest';
import { pressedOutside } from './outside';

afterEach(() => document.body.replaceChildren());

function el(html: string): HTMLElement {
	const holder = document.createElement('div');
	holder.innerHTML = html;
	document.body.append(holder);
	return holder;
}

describe('a press outside a floating element', () => {
	it('is outside on the page, inside in the element or its trigger', () => {
		const page = el('<button id="trigger">History</button><div id="panel"><button id="row">v3</button></div><p id="elsewhere">Text</p>');
		const [trigger, panel] = [page.querySelector<HTMLElement>('#trigger'), page.querySelector<HTMLElement>('#panel')];
		expect(pressedOutside(page.querySelector('#elsewhere'), [trigger, panel])).toBe(true);
		expect(pressedOutside(page.querySelector('#row'), [trigger, panel])).toBe(false);
		expect(pressedOutside(trigger, [trigger, panel])).toBe(false);
	});

	it('is not outside in a dialog opened over it, nor on that dialog\'s backdrop', () => {
		const page = el('<button id="trigger">History</button><div id="panel">Versions</div>');
		const dialog = el(
			'<div class="modal-backdrop" id="backdrop"><div role="dialog" aria-modal="true"><button id="confirm">Restore</button></div></div>',
		);
		const elements = [page.querySelector<HTMLElement>('#trigger'), page.querySelector<HTMLElement>('#panel')];
		expect(pressedOutside(dialog.querySelector('#confirm'), elements)).toBe(false);
		expect(pressedOutside(dialog.querySelector('#confirm')!.firstChild, elements)).toBe(false);
		expect(pressedOutside(dialog.querySelector('#backdrop'), elements)).toBe(false);
	});

	it('is outside elsewhere in the dialog a popover sits in', () => {
		const dialog = el(
			'<div class="modal-backdrop"><div role="dialog" aria-modal="true"><div id="menu">Options</div><p id="body">Body</p></div></div>',
		);
		expect(pressedOutside(dialog.querySelector('#body'), [dialog.querySelector<HTMLElement>('#menu')])).toBe(true);
	});
});
