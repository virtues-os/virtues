/**
 * Device pages live in Sources for devices that send data (owner,
 * 2026-11-20). These check which devices go there and which stay in Settings.
 */
import { describe, expect, it, vi } from 'vitest';

vi.mock('$lib/stores/dialog.svelte', () => ({ confirmAction: vi.fn() }));
vi.mock('svelte-sonner', () => ({ toast: { error: vi.fn() } }));

const { sourcePageOf } = await import('./shared');

type Row = {
	id: string;
	kind: 'mobile_app' | 'desktop_app' | 'sensor' | 'cli' | 'agent';
	source_id: string | null;
	installed_by: string | null;
	is_current: boolean;
};

const row = (over: Partial<Row>): Row => ({
	id: 'dev_a',
	kind: 'desktop_app',
	source_id: '__device__',
	installed_by: null,
	is_current: false,
	...over
});

describe('where a device page lives', () => {
	it('sends an iPhone you are not using to Sources', () => {
		const phone = row({ kind: 'mobile_app', source_id: 'ios' });
		expect(sourcePageOf(phone, [phone])).toBe('/sources/ios');
	});

	it('sends a Mac app with a collector, and the collector, to Sources', () => {
		const app = row({ id: 'dev_app' });
		const collector = row({ id: 'dev_col', source_id: 'mac', installed_by: 'dev_app' });
		expect(sourcePageOf(app, [app, collector])).toBe('/sources/mac');
		expect(sourcePageOf(collector, [app, collector])).toBe('/sources/mac');
	});

	it('keeps the device you are using, and devices that send nothing, in Settings', () => {
		const here = row({ kind: 'mobile_app', source_id: 'ios', is_current: true });
		const agent = row({ kind: 'agent' });
		const windows = row({ id: 'dev_win' });
		expect(sourcePageOf(here, [here])).toBeNull();
		expect(sourcePageOf(agent, [agent])).toBeNull();
		expect(sourcePageOf(windows, [windows])).toBeNull();
	});
});
