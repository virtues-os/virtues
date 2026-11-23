// The addresses of sites and jobs. Every name here is invented.
import { describe, expect, it } from 'vitest';
import { homeRoute, optionsRoute, readAddress } from './routes';

describe('a site or server task address', () => {
	it('says the word for its kind when the kind is known', () => {
		expect(homeRoute('applet_star_map', 'site')).toBe('/site/star_map');
		expect(homeRoute('applet_meeting_prep', 'skill')).toBe('/skill/meeting_prep');
		expect(homeRoute('applet_user__tides', 'server_task')).toBe('/server-task/tides');
		expect(optionsRoute('applet_star_map', null, 'site')).toBe('/site/star_map/options');
	});

	it('shows the name without the id prefix, and goes by whose it is when the kind is not known', () => {
		expect(homeRoute('applet_user__water_log')).toBe('/site/water_log');
		expect(homeRoute('applet_embedding_index')).toBe('/server-task/embedding_index');
		expect(optionsRoute('applet_user__water_log')).toBe('/site/water_log/options');
		expect(optionsRoute('applet_embedding_index', 'history')).toBe('/server-task/embedding_index/options?panel=history');
	});

	it('reads back to the same id and side', () => {
		expect(readAddress('/site/water_log')).toEqual({
			id: 'applet_user__water_log',
			otherId: 'applet_water_log',
			word: 'site',
			side: 'home',
			open: null,
			route: '/site/water_log',
		});
		expect(readAddress('/server-task/embedding_index/options?panel=details')).toEqual({
			id: 'applet_embedding_index',
			otherId: 'applet_user__embedding_index',
			word: 'server-task',
			side: 'options',
			open: 'details',
			route: '/server-task/embedding_index/options?panel=details',
		});
		expect(readAddress('/skill/meeting_prep')).toMatchObject({ id: 'applet_user__meeting_prep', otherId: 'applet_meeting_prep', word: 'skill' });
	});

	it('reads the addresses used before', () => {
		expect(readAddress('/applet/applet_user__water_log/view')?.route).toBe('/site/water_log');
		expect(readAddress('/applet/applet_user__water_log')?.route).toBe('/site/water_log');
		expect(readAddress('/applet/applet_user__water_log?panel=info')?.route).toBe('/site/water_log/options');
		expect(readAddress('/applet/applet_embedding_index?panel=history')?.route).toBe('/server-task/embedding_index/options?panel=history');
		expect(readAddress('/action/applet_embedding_index')?.route).toBe('/server-task/embedding_index');
		expect(readAddress('/job/embedding_index/options')?.route).toBe('/server-task/embedding_index/options');
	});

	it('is null for anything else', () => {
		expect(readAddress('/sites')).toBeNull();
		expect(readAddress('/site/water_log/runs')).toBeNull();
		expect(readAddress('/page/page_1')).toBeNull();
	});
});
