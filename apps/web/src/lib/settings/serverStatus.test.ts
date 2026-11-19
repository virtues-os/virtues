// The opening sentence on Settings · Your server names the first thing that
// needs you, in a fixed order, and the row values say where each part stands.
import { describe, expect, it } from 'vitest';
import type { BackupStatus, SearchStatus, UpdateStatus } from '$lib/api/client';
import { appletsValue, dataDisk, formatBytes, searchValue, serverLead, updatesValue, upFor, type LeadInput } from './serverStatus';

const GB = 1024 ** 3;
const healthy = { status: 'healthy', database: 'connected' };
const roomy = { total: 500 * GB, available: 300 * GB };
const full = { total: 500 * GB, available: 20 * GB };
const backup = (state: string, age_seconds: number | null = 4 * 3600): BackupStatus => ({ state, age_seconds, volumes: [] });

const well: LeadInput = { health: healthy, disk: roomy, backup: backup('ok'), appletsNeedingYou: [] };

describe('the opening sentence', () => {
	it('says the server is running well, with the facts it read', () => {
		const lead = serverLead(well);
		expect(lead.kind).toBe('well');
		expect(lead.sentence).toBe('Your server is running well.');
		expect(lead.sub).toBe('Its disk has plenty of room, and its last backup finished 4 hours ago.');
	});

	it('leaves out a fact it could not read', () => {
		expect(serverLead({ ...well, backup: null }).sub).toBe('Its disk has plenty of room.');
		expect(serverLead({ ...well, disk: null, backup: null }).sub).toBe('Everything it checks is answering.');
	});

	it('puts the database first', () => {
		const lead = serverLead({ ...well, health: { status: 'unhealthy', database: 'error' }, disk: full, backup: backup('failing') });
		expect(lead.kind).toBe('database');
		expect(lead.guide?.href).toContain('/docs/operate/recovery');
	});

	it('puts an almost full disk before backups and applets', () => {
		const lead = serverLead({ ...well, disk: full, backup: backup('none'), appletsNeedingYou: ['Garden watering'] });
		expect(lead.kind).toBe('disk');
		expect(lead.sub).toBe('20 GB of 500 GB is left. A shorter time under Keep recordings deletes older audio once it is transcribed.');
		expect(lead.action?.route).toBe('/virtues/data');
	});

	it('puts a failed backup before a missing one, and both before applets', () => {
		expect(serverLead({ ...well, backup: backup('failing'), appletsNeedingYou: ['Garden watering'] }).kind).toBe('backup-failing');
		expect(serverLead({ ...well, backup: backup('none') }).kind).toBe('backup-none');
		expect(serverLead({ ...well, backup: backup('never', null) }).kind).toBe('backup-never');
		expect(serverLead({ ...well, backup: backup('stale', 9 * 86_400) }).sentence).toBe('Your last backup finished 9 days ago.');
	});

	it('names one applet that needs you, and does not count several', () => {
		expect(serverLead({ ...well, appletsNeedingYou: ['Garden watering'] }).sentence).toBe('Garden watering needs you.');
		expect(serverLead({ ...well, appletsNeedingYou: ['Garden watering', 'Reading list'] }).sentence).toBe('Some of your applets need you.');
	});

	it('says how much is left on a fuller disk that is not yet almost full', () => {
		expect(serverLead({ ...well, disk: { total: 500 * GB, available: 80 * GB } }).sub).toMatch(/^Its disk has 80 GB left/);
	});
});

describe('the row values', () => {
	const update = (over: Partial<UpdateStatus>): UpdateStatus => ({
		current: '0.1.4',
		channel: 'stable',
		running_version: 'v0.1.4',
		running_channel: 'stable',
		running_ahead: false,
		latest: 'v0.1.4',
		update_available: false,
		staged: null,
		check_error: null,
		...over,
	});

	it('says where updates stand', () => {
		expect(updatesValue(null)).toBeUndefined();
		expect(updatesValue(update({}))).toBe('Up to date');
		expect(updatesValue(update({ update_available: true, latest: 'v0.1.5' }))).toBe('v0.1.5 is available');
		expect(updatesValue(update({ update_available: true, latest: 'v0.1.5', staged: { slot_id: 'v0.1.5-abc1234', version: 'v0.1.5', sha: null } as UpdateStatus['staged'] }))).toBe('v0.1.5 is ready');
		expect(updatesValue(update({ running_ahead: true }))).toBe('Early build');
		expect(updatesValue(update({ check_error: 'offline' }))).toBe('Couldn’t check');
	});

	it('says whether search answers', () => {
		const search = (reachable: boolean, busy: boolean) => ({ reachable, busy }) as SearchStatus;
		expect(searchValue(search(true, false))).toBe('Answering');
		expect(searchValue(search(false, true))).toBe('Busy');
		expect(searchValue(search(false, false))).toBe('Not answering');
	});

	it('names an applet that needs you', () => {
		expect(appletsValue(null)).toBeUndefined();
		expect(appletsValue([])).toBe('Running on schedule');
		expect(appletsValue(['Garden watering'])).toBe('Garden watering needs you');
		expect(appletsValue(['Garden watering', 'Reading list'])).toBe('Some need you');
	});
});

describe('the small formats', () => {
	it('picks the disk that holds your data', () => {
		const disks = [
			{ mount: '/media/drive', total: 2000, available: 10 },
			{ mount: '/', total: 900, available: 100 },
		];
		expect(dataDisk(disks)?.mount).toBe('/');
		expect(dataDisk([{ mount: '/data', total: 5, available: 1 }, { mount: '/boot', total: 1, available: 1 }])?.mount).toBe('/data');
		expect(dataDisk([])).toBeNull();
	});

	it('writes sizes and how long it has been up', () => {
		expect(formatBytes(300 * GB)).toBe('300 GB');
		expect(formatBytes(1.5 * GB)).toBe('1.5 GB');
		expect(upFor(24 * 86_400 + 3600)).toBe('24 days');
		expect(upFor(86_400)).toBe('1 day');
		expect(upFor(3 * 3600)).toBe('3 hours');
		expect(upFor(30)).toBe('1 minute');
	});
});
