/**
 * Settings · This Mac. Which device this is comes with the device list, after
 * mount, so whatever the page reads from the Mac app waits for it.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

const bridge = vi.hoisted(() => ({
	getCollectorStatus: vi.fn(),
	recheckCollector: vi.fn(),
	pauseCollector: vi.fn(),
	resumeCollector: vi.fn(),
	shellIdentity: vi.fn(),
	appUpdateState: vi.fn(),
	applyAppUpdate: vi.fn(),
	checkAppUpdate: vi.fn(),
}));
vi.mock('$lib/tauri/bridge', async (original) => ({ ...(await original<object>()), ...bridge }));
vi.mock('$lib/utils/platform', async (original) => ({ ...(await original<object>()), isTauri: true, isMacOS: true }));
const client = vi.hoisted(() => ({ listDevices: vi.fn() }));
vi.mock('$lib/api/client', async (original) => ({ ...(await original<object>()), ...client }));
vi.mock('$lib/components/devices/MacPermissions.svelte', () => ({ default: () => {} }));

const { default: DeviceView } = await import('./DeviceView.svelte');

const status = (over: object = {}) => ({
	version: '1.0.33',
	running: true,
	paused: false,
	pendingEvents: 0,
	pendingMessages: 0,
	lastSync: null,
	hasFullDiskAccess: true,
	hasAccessibility: false,
	permissionsReportedByDaemon: true,
	permissionsCheckedAt: null,
	safariLibrary: true,
	...over,
});

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;
const text = () => (target.textContent ?? '').replace(/\s+/g, ' ');

async function open() {
	mounted = mount(DeviceView, { target, props: { deviceId: 'this' } });
	for (let i = 0; i < 5; i++) {
		await new Promise((r) => setTimeout(r, 0));
		flushSync();
	}
}

beforeEach(() => {
	vi.clearAllMocks();
	client.listDevices.mockResolvedValue({
		devices: [{ id: 'dev_x', kind: 'desktop_app', label: 'Virtues Desktop', is_current: true, paired_at: '2026-11-01T00:00:00Z' }],
	});
	bridge.getCollectorStatus.mockResolvedValue(status());
	bridge.shellIdentity.mockResolvedValue({ appVersion: '1.0.33', commandSurface: 1, activeBundle: null, servingBundle: null, lastCheck: null });
	bridge.appUpdateState.mockResolvedValue({ stagedVersion: null, lastCheck: { outcome: 'up_to_date' } });
	target = document.createElement('div');
	document.body.appendChild(target);
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	target.remove();
});

describe('This Mac', () => {
	it('is titled This Mac, not by the name the device paired under', async () => {
		await open();
		expect(target.querySelector('h1')?.textContent?.trim()).toBe('This Mac');
	});

	it("shows the app's version", async () => {
		await open();
		expect(text()).toContain('Virtues 1.0.33');
		expect(text()).toContain('Up to date');
	});

	it('keeps build numbers closed until asked for', async () => {
		await open();
		expect(text()).not.toContain('Channel');
		[...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Version details')!.click();
		flushSync();
		expect(text()).toContain('Channel');
	});

	it('says when the collector is older than the app without opening the details', async () => {
		bridge.getCollectorStatus.mockResolvedValue(status({ version: '1.0.31' }));
		await open();
		expect(text()).toContain('Its collector is older than the app.');
	});

	it('says what is waiting only while something is', async () => {
		await open();
		expect(text()).toContain('App activity and messages are all sent.');
		unmount(mounted!);
		bridge.getCollectorStatus.mockResolvedValue(status({ pendingEvents: 12, pendingMessages: 7 }));
		await open();
		expect(text()).toContain('Sending 19 items.');
	});

	it('does not say it is sending while paused', async () => {
		bridge.getCollectorStatus.mockResolvedValue(status({ paused: true, pendingEvents: 1 }));
		await open();
		expect(text()).toContain('Paused');
		expect(text()).toContain('1 item waiting to send.');
	});
});
