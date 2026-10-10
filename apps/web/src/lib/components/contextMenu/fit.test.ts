// @vitest-environment happy-dom
/**
 * A menu opened at a point on a phone, the software keyboard up: it opens
 * above the keys, not behind them.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
	(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
});

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { contextMenu } from '$lib/stores/contextMenu.svelte';
import { fitMenu, menuRoom } from './fit';
import { pointFocusAt } from './activeDescendant';
import { listenForMenuKeys } from './keys';

const HERE = path.dirname(fileURLToPath(import.meta.url));

afterEach(() => {
	document.documentElement.style.removeProperty('--keyboard-inset');
	contextMenu.hide();
});

describe('a menu opened at a point', () => {
	it('opens above the software keyboard, which the window’s height reaches under', () => {
		window.innerHeight = 852;
		window.innerWidth = 393;
		document.documentElement.style.setProperty('--keyboard-inset', '336px');
		expect(menuRoom()).toEqual({ width: 393, height: 516 });
		contextMenu.show({ x: 40, y: 450 }, [{ id: 'a', label: 'Row above' }]);
		expect(contextMenu.position.y + 300 + 8).toBeLessThanOrEqual(516);

		// Once drawn, by its real size: a long one sits as high as it must.
		expect(fitMenu({ x: 40, y: 450 }, { width: 220, height: 480 })).toEqual({ x: 40, y: 28 });
		expect(fitMenu({ x: 300, y: 100 }, { width: 220, height: 120 })).toEqual({ x: 165, y: 100 });

		// With no keyboard, the whole window.
		document.documentElement.style.removeProperty('--keyboard-inset');
		contextMenu.show({ x: 40, y: 450 }, [{ id: 'a', label: 'Row above' }]);
		expect(contextMenu.position.y).toBe(450);
	});

	it('is fitted again by its real size once drawn, and never runs under the keys', () => {
		const provider = fs.readFileSync(path.join(HERE, 'ContextMenuProvider.svelte'), 'utf8');
		expect(provider).toMatch(/if \(contextMenu\.anchor\) return;\s+\/\/[^\n]*\n\s+const size = \{ width: menuRef\.offsetWidth, height: menuRef\.offsetHeight \};\s+const fitted = fitMenu\(contextMenu\.position, size\);/);
		expect(provider).toMatch(/100dvh - var\(--keyboard-inset, 0px\)/);
	});
});

// Opened by Shift-F10 with no row reached, a menu said nothing to a screen
// reader: the focus named no row until the arrows moved.
describe('a menu opened from the keyboard', () => {
	it('starts on its first row that can be chosen, which the focus then names', () => {
		const owner = document.createElement('div');
		owner.setAttribute('contenteditable', 'true');
		document.body.append(owner);
		const pointed = pointFocusAt(owner, 'context-menu');
		contextMenu.show({ x: 40, y: 100 }, [
			{ id: 'off', label: 'Unavailable', disabled: true },
			{ id: 'open', label: 'Open' },
			{ id: 'copy', label: 'Copy link' },
		]);
		expect(contextMenu.focusedIndex).toBe(-1);
		contextMenu.reachFirstRow();
		expect(contextMenu.focusedIndex).toBe(1);
		pointed.highlight(`context-menu-row-${contextMenu.focusedIndex}`);
		expect(owner.getAttribute('aria-activedescendant')).toBe('context-menu-row-1');
		// Reached already, or closed: nothing moves.
		contextMenu.focusNext();
		contextMenu.reachFirstRow();
		expect(contextMenu.focusedIndex).toBe(2);
		contextMenu.hide();
		contextMenu.reachFirstRow();
		expect(contextMenu.focusedIndex).toBe(-1);
		pointed.release();
	});

	it('takes End, Home and a letter to its rows, skipping one that cannot be chosen', () => {
		const stop = listenForMenuKeys(contextMenu);
		try {
			contextMenu.show({ x: 40, y: 100 }, [
				{ id: 'open', label: 'Open' },
				{ id: 'beside', label: 'Open beside' },
				{ id: 'copy', label: 'Copy link', disabled: true },
				{ id: 'remove', label: 'Remove link' },
			]);
			const key = (k: string) => window.dispatchEvent(new KeyboardEvent('keydown', { key: k, cancelable: true }));
			key('End');
			expect(contextMenu.focusedIndex).toBe(3);
			key('Home');
			expect(contextMenu.focusedIndex).toBe(0);
			key('o');
			expect(contextMenu.focusedIndex).toBe(1);
			key('c');
			expect(contextMenu.focusedIndex).toBe(1);
		} finally {
			stop();
		}
	});
});
