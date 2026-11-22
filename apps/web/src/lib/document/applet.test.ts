import { describe, expect, it, vi } from 'vitest';

vi.mock('$lib/api/client', () => ({ getApplet: vi.fn() }));

import { appletBlockText } from './applet';

describe('appletBlockText', () => {
	it("writes the contract's applet tag with the applet's id", () => {
		expect(appletBlockText('water-log')).toBe('<virtues-applet ref="water-log"></virtues-applet>');
	});

	it('keeps an id from ending the attribute', () => {
		expect(appletBlockText('a"b&c')).toBe('<virtues-applet ref="a&quot;b&amp;c"></virtues-applet>');
	});
});
