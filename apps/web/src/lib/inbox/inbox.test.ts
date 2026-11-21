import { describe, expect, it } from 'vitest';
import type { AttentionItem, Note, Task } from '$lib/api/client';
import type { TimelineDay } from '$lib/timeline/day';
import { buildItems, noteRing, questionItem, taskRing } from './items';
import { placeQuestion, type PlaceQuestion } from './questions';
import { buildGraph, hubId } from './graph';
import { fitLabels, labelWidth, markBox } from './labels';
import { CX, CY, LOOSE, RING_R, layout, placeLabels, polar, slots, type Dot } from './radar';
import { CLOCK_X0, MAP_H, MAP_W, buildRoute, clockScale, dayBars, dayBounds, entriesAhead, geoBox, itemTime, stack } from './route';

// An invented clock: a Thursday, early afternoon.
const NOW = new Date(2026, 2, 12, 13, 30);

function task(over: Partial<Task>): Task {
	return {
		id: 'task_1',
		title: 'Order a new chain',
		notes: null,
		assignee: 'owner',
		status: 'open',
		due_on: null,
		due_at: null,
		thread_id: null,
		origin: 'owner',
		source_url: null,
		sort_order: 0,
		closed_at: null,
		created_at: '2026-03-10T15:00:00Z',
		updated_at: '2026-03-10T15:00:00Z',
		result: null,
		result_url: null,
		chat_id: null,
		next_run_at: null,
		attempts: 0,
		...over,
	};
}

describe('rings', () => {
	it('puts a task on the ring of its due day', () => {
		expect(taskRing({ status: 'open', due_on: '2026-03-12' }, NOW)).toBe(1);
		expect(taskRing({ status: 'open', due_on: '2026-03-13' }, NOW)).toBe(2);
		expect(taskRing({ status: 'open', due_on: '2026-03-25' }, NOW)).toBe(3);
		expect(taskRing({ status: 'open', due_on: null }, NOW)).toBe(3);
	});

	it('keeps a task from an earlier day on Today', () => {
		expect(taskRing({ status: 'open', due_on: '2026-03-10' }, NOW)).toBe(1);
	});

	it('puts a result that waits to be accepted on Now', () => {
		expect(taskRing({ status: 'ready', due_on: null }, NOW)).toBe(0);
	});

	it('puts a note on the ring of the day it stops mattering', () => {
		expect(noteRing({ until: new Date(2026, 2, 12, 18).toISOString() }, NOW)).toBe(1);
		expect(noteRing({ until: new Date(2026, 2, 14, 5).toISOString() }, NOW)).toBe(2);
		expect(noteRing({ until: new Date(2026, 3, 12).toISOString() }, NOW)).toBe(3);
	});
});

describe('buildItems', () => {
	const note: Note = {
		key: 'clash:a:b',
		kind: 'clash',
		text: 'Bike shop pickup and Garden center visit overlap on Saturday.',
		subjects: ['/thread/t_bike', '/thread/t_garden'],
		about: 'Fix the bike',
		when: 'Saturday at 10:00 AM',
		action: 'See the day',
		action_route: '/day/2026-03-14',
		until: new Date(2026, 2, 14, 10).toISOString(),
	};
	const alert: AttentionItem = { key: 'backup', kind: 'backup', title: 'Your backup has not run', body: 'Its disk is full.', route: '/virtues/box' };

	it('lists what your server needs first, then by ring', () => {
		const items = buildItems({
			tasks: [task({ id: 'a', due_on: '2026-03-13' }), task({ id: 'b', title: 'Buy seed trays', due_on: '2026-03-12', thread_id: 't_garden' })],
			notes: [note],
			attention: [alert],
			active: new Set(['t_garden']),
			now: NOW,
		});
		expect(items.map((i) => i.key)).toEqual(['attention:backup', 'task:b', 'note:clash:a:b', 'task:a']);
		expect(items.map((i) => i.kind)).toEqual(['needs', 'task', 'insight', 'task']);
	});

	it('files a note under its first thread, and shows a thread that is not active as none', () => {
		const active = buildItems({ tasks: [], notes: [note], attention: [], active: new Set(['t_bike']), now: NOW });
		expect(active[0].threadId).toBe('t_bike');
		const parked = buildItems({ tasks: [task({ thread_id: 't_bike' })], notes: [], attention: [], active: new Set(), now: NOW });
		expect(parked[0].threadId).toBeNull();
	});

	it('marks a request the assistant has not finished as waiting', () => {
		const items = buildItems({ tasks: [task({ assignee: 'assistant', status: 'active' })], notes: [], attention: [], active: new Set(), now: NOW });
		expect(items[0].kind).toBe('waiting');
	});

	it('places a thing from signal to noise by who is waiting, its day and where it sits', () => {
		const items = buildItems({
			tasks: [
				task({ id: 'a', title: 'Compare two chain brands', assignee: 'assistant', status: 'ready' }),
				task({ id: 'b', title: 'True the front wheel', thread_id: 't_bike' }),
				task({ id: 'c', title: 'Return the library book', due_on: '2026-03-12' }),
				task({ id: 'd', title: 'Look up torque values', assignee: 'assistant', status: 'active' }),
				task({ id: 'e', title: 'Sharpen the kitchen knives', due_on: '2026-03-25' }),
			],
			notes: [note],
			attention: [alert],
			active: new Set(['t_bike']),
			now: NOW,
		});
		const noise = Object.fromEntries(items.map((it) => [it.key, it.noise]));
		// A result to accept and what your server noticed show wherever the control is.
		expect(noise['task:a']).toBe(0);
		expect(noise['note:clash:a:b']).toBe(0);
		// Then a task due today, then one in a thread you follow, then the rest.
		expect(noise['task:c']).toBeGreaterThan(0);
		expect(noise['task:c']).toBeLessThan(noise['task:b']);
		expect(noise['task:b']).toBeLessThan(Math.min(noise['task:e'], noise['task:d'], noise['attention:backup']));
		// The rest are spread evenly, and the last one is at the noise end.
		const rest = Object.values(noise).filter((n) => n > 0).sort((x, y) => x - y);
		expect(rest).toEqual([1, 2, 3, 4, 5].map((i) => i / 5));
	});

	it('keeps an unlisted thread toward the noise end until its day comes', () => {
		const build = (carDue: string) =>
			Object.fromEntries(
				buildItems({
					tasks: [
						task({ id: 'bike', title: 'True the front wheel', thread_id: 't_bike' }),
						task({ id: 'soon', title: 'Order brake pads', thread_id: 't_bike', due_on: '2026-03-14' }),
						task({ id: 'car', title: 'Book the oil change', thread_id: 't_car', due_on: carDue }),
						task({ id: 'loose', title: 'Sharpen the kitchen knives' }),
					],
					notes: [{ ...note, key: 'car', subjects: ['/thread/t_car'], until: new Date(2026, 3, 12).toISOString() }, note],
					attention: [],
					active: new Set(['t_bike', 't_car']),
					unlisted: new Set(['t_car']),
					now: NOW,
				}).map((it) => [it.key, it]),
			);
		const far = build('2026-03-25');
		// It keeps its thread's name, and sits after everything in a listed thread.
		expect(far['task:car'].threadId).toBe('t_car');
		expect(far['task:car'].noise).toBeGreaterThan(Math.max(far['task:bike'].noise, far['task:soon'].noise));
		// What your server noticed about it waits too. In a listed thread it shows at once.
		expect(far['note:car'].noise).toBeGreaterThan(far['task:bike'].noise);
		expect(far['note:clash:a:b'].noise).toBe(0);
		// A listed thread's thing due this week comes before one with no day.
		expect(far['task:soon'].noise).toBeLessThan(far['task:bike'].noise);
		// Later that week it passes the things with no thread, and on its day it leads.
		const week = build('2026-03-14');
		expect(week['task:car'].noise).toBeLessThan(week['task:loose'].noise);
		expect(week['task:car'].noise).toBeGreaterThan(week['task:bike'].noise);
		const due = build('2026-03-12');
		expect(due['task:car'].noise).toBeLessThan(Math.min(due['task:soon'].noise, due['task:bike'].noise));
	});
});

describe('radar layout', () => {
	const threads = [
		{ id: 'bike', name: 'Fix the bike', group: 'The shed' },
		{ id: 'garden', name: 'Plan the garden', group: null },
		{ id: 'shelves', name: 'Build the shelves', group: 'The shed' },
	];

	it('gives the whole circle to what is in no thread when there are no threads', () => {
		const lay = layout([]);
		expect(lay.lanes[LOOSE].center).toBe(180);
		expect(lay.loose).toEqual({ from: 0, width: 360 });
		expect(lay.rim).toEqual([]);
	});

	it('shares the circle between the threads and the loose wedge', () => {
		const lay = layout(threads);
		const total = Object.values(lay.lanes).reduce((sum, l) => sum + l.width, 0);
		expect(total).toBeCloseTo(360);
		expect(lay.lanes[LOOSE].center).toBe(180);
	});

	it('puts threads under the same thing side by side, with one label for the thing', () => {
		const lay = layout(threads);
		// Angles wrap at the top of the circle, so measure the short way round.
		const apart = Math.abs(lay.lanes.bike.center - lay.lanes.shelves.center) % 360;
		expect(Math.min(apart, 360 - apart)).toBeCloseTo(lay.lanes.bike.width);
		expect(lay.rim.filter((r) => r.caps === 'The shed')).toHaveLength(1);
		expect(lay.arcs.map((a) => a.key)).toEqual(['The shed']);
	});

	it('names a thing on the middle label of its group, never on a label of its own', () => {
		const three = layout([
			{ id: 'bike', name: 'Fix the bike', group: 'The shed' },
			{ id: 'shelves', name: 'Build the shelves', group: 'The shed' },
			{ id: 'kite', name: 'Mend the kite', group: 'The shed' },
		]);
		expect(three.rim.map((r) => [r.threadId, r.caps])).toEqual([
			['bike', ''],
			['shelves', 'The shed'],
			['kite', ''],
			[LOOSE, ''],
		]);
	});

	it('puts the ring names on the line between two slices nearest the top', () => {
		const lay = layout(threads);
		expect(lay.bounds.map((b) => b.at)).toContain(lay.scaleAt);
		const beside = Object.values(lay.lanes).filter((l) => l.scale);
		expect(beside.map((l) => l.scale).sort()).toEqual(['end', 'start']);
	});
});

describe('radar slots and labels', () => {
	const lay = layout([
		{ id: 'bike', name: 'Fix the bike', group: null },
		{ id: 'garden', name: 'Plan the garden', group: null },
	]);

	it('keeps marks that share a slice and a ring apart', () => {
		const dots: Dot[] = Array.from({ length: 9 }, (_, i) => ({ key: `d${i}`, lane: 'bike', ring: 1, size: 14 }));
		const spots = Object.values(slots(dots, lay));
		for (let i = 0; i < spots.length; i++) {
			for (let j = i + 1; j < spots.length; j++) {
				expect(Math.hypot(spots[i].x - spots[j].x, spots[i].y - spots[j].y)).toBeGreaterThan(15);
			}
		}
	});

	it('puts a thing in an unknown thread in the loose wedge', () => {
		const spot = slots([{ key: 'x', lane: 'gone', ring: 0, size: 14 }], lay).x;
		expect(spot.ang).toBe(180);
		expect(spot.x).toBeCloseTo(CX);
		expect(spot.y).toBeCloseTo(CY + RING_R[0]);
	});

	it('staggers rows of different lengths, so stacked marks never touch', () => {
		// Enough marks for two rows, with one more in the first row than the second.
		const many: Dot[] = Array.from({ length: 13 }, (_, i) => ({ key: `d${i}`, lane: 'bike', ring: 0, size: 14 }));
		const spots = Object.values(slots(many, lay));
		const radii = new Set(spots.map((s) => Math.round(Math.hypot(s.x - CX, s.y - CY))));
		expect(radii.size).toBeGreaterThan(1);
		for (let i = 0; i < spots.length; i++) {
			for (let j = i + 1; j < spots.length; j++) {
				expect(Math.hypot(spots[i].x - spots[j].x, spots[i].y - spots[j].y)).toBeGreaterThan(20);
			}
		}
	});

	it('keeps marks clear of the ring names in the slices either side of them', () => {
		const start = Object.entries(lay.lanes).find(([, l]) => l.scale === 'start');
		expect(start).toBeDefined();
		const [id, lane] = start ?? ['', { center: 0, width: 0 }];
		// One mark sits at the middle of the room left, which is past the middle of the slice.
		const spot = slots([{ key: 'x', lane: id, ring: 0, size: 14 }], lay).x;
		expect(spot.ang).toBeGreaterThan(lane.center);
	});

	it('keeps a label off a thread\'s name on the rim', () => {
		const rim = lay.rim.find((r) => r.threadId === 'bike');
		expect(rim).toBeDefined();
		if (!rim) return;
		// The name hangs to the left of its point. A mark just right of the
		// point would put its label over the name, so the label goes to the
		// mark's other side.
		const placed = placeLabels([{ key: 'p', text: 'Oil the hinges', x: rim.x + 40, y: rim.y, size: 14, ang: 270, pinned: false }], [], lay).p;
		expect(placed).not.toBeNull();
		expect(placed?.x).toBeGreaterThan(rim.x);
	});

	it('places labels that cover neither each other nor a mark', () => {
		const dots: Dot[] = Array.from({ length: 5 }, (_, i) => ({ key: `d${i}`, lane: 'bike', ring: 1, size: 14 }));
		const spots = slots(dots, lay);
		const marks = dots.map((d) => ({ x: spots[d.key].x, y: spots[d.key].y, size: d.size }));
		const text = 'Return the borrowed pump';
		const placed = placeLabels(
			dots.map((d) => ({ key: d.key, text, x: spots[d.key].x, y: spots[d.key].y, size: d.size, ang: spots[d.key].ang, pinned: false })),
			marks,
		);
		const boxes = Object.values(placed)
			.filter((b) => b !== null)
			.map((b) => ({ x1: b.x, y1: b.y, x2: b.x + labelWidth(text), y2: b.y + 20 }));
		expect(boxes.length).toBeGreaterThan(0);
		for (let i = 0; i < boxes.length; i++) {
			for (let j = i + 1; j < boxes.length; j++) {
				const apart = boxes[i].x2 <= boxes[j].x1 || boxes[j].x2 <= boxes[i].x1 || boxes[i].y2 <= boxes[j].y1 || boxes[j].y2 <= boxes[i].y1;
				expect(apart).toBe(true);
			}
			for (const m of marks) {
				const clear = boxes[i].x2 <= m.x - m.size / 2 || boxes[i].x1 >= m.x + m.size / 2 || boxes[i].y2 <= m.y - m.size / 2 || boxes[i].y1 >= m.y + m.size / 2;
				expect(clear).toBe(true);
			}
		}
	});

	it('always labels the thing you picked', () => {
		const crowded = Array.from({ length: 30 }, (_, i) => ({ x: 390 + i, y: 300, size: 14 }));
		const placed = placeLabels([{ key: 'p', text: 'Oil the hinges', x: 390, y: 300, size: 14, ang: 0, pinned: true }], crowded);
		expect(placed.p).not.toBeNull();
	});
});

describe('today as a route', () => {
	// An invented day in an invented spot: three stops a few hundred metres apart.
	const at = (h: number, m = 0) => new Date(2026, 2, 12, h, m).getTime();
	const iso = (h: number, m = 0) => new Date(at(h, m)).toISOString();
	const stay = (id: string, name: string, lat: number, lng: number, from: number, to: number) => ({
		id,
		started_at: iso(from),
		ended_at: iso(to),
		latitude: lat,
		longitude: lng,
		place_id: `place_${id}`,
		place_name: name,
		place_is_named: true,
	});
	const event = (id: string, title: string, from: number, to: number, location_name: string | null) => ({
		id,
		title,
		started_at: iso(from),
		ended_at: iso(to),
		is_all_day: false,
		calendar_name: null,
		location_name,
		status: null,
		response_status: null,
	});
	function day(over: Partial<TimelineDay>): TimelineDay {
		return {
			date: '2026-03-12',
			zone: Intl.DateTimeFormat().resolvedOptions().timeZone,
			started_at: iso(0),
			ended_at: new Date(2026, 2, 13).toISOString(),
			stays: [],
			points: [],
			place_titles: {},
			last_point_before: null,
			nights: [],
			sessions: [],
			steps: [],
			step_scale: 1,
			calendar: [],
			...over,
		};
	}
	const stays = [stay('a', 'The boathouse', 10.0, 20.0, 7, 9), stay('b', 'Corner bakery', 10.004, 20.006, 9, 12), stay('c', 'The workshop', 10.002, 20.012, 12, 14)];

	it('has no route on a day with no location', () => {
		const route = buildRoute(day({}), NOW.getTime());
		expect(route.you).toBeNull();
		expect(route.stops).toEqual([]);
		expect(route.at(at(9))).toBeNull();
	});

	it('draws the stops in the box with north up, and puts you at the last one', () => {
		const route = buildRoute(day({ stays }), NOW.getTime());
		const [boat, bakery, shop] = route.stops;
		for (const s of route.stops) {
			expect(s.x).toBeGreaterThan(0);
			expect(s.x).toBeLessThan(MAP_W);
			expect(s.y).toBeGreaterThan(0);
			expect(s.y).toBeLessThan(MAP_H);
		}
		expect(bakery.y).toBeLessThan(boat.y);
		expect(shop.x).toBeGreaterThan(bakery.x);
		expect(route.runs).toHaveLength(1);
		expect(route.ahead).toBe('');
		expect(route.you).toEqual({ x: shop.x, y: shop.y });
		expect(route.at(at(8))).toEqual({ x: boat.x, y: boat.y });
	});

	it('draws on the map\'s own ground when a map is under it', () => {
		const route = buildRoute(day({ stays }), NOW.getTime(), (lat, lng) => [lng * 10_000, lat * 10_000]);
		const want = [
			[200_000, 100_000],
			[200_060, 100_040],
			[200_120, 100_020],
		];
		expect(route.stops).toHaveLength(3);
		route.stops.forEach((s, i) => {
			expect(s.x).toBeCloseTo(want[i][0], 3);
			expect(s.y).toBeCloseTo(want[i][1], 3);
		});
	});

	it('gives the ground the day covers so far, and none on a day with no location', () => {
		expect(geoBox(day({}), NOW.getTime())).toBeNull();
		expect(geoBox(day({ stays }), at(10))).toEqual({ lat0: 10.0, lat1: 10.004, lng0: 20.0, lng1: 20.006 });
	});

	it('leaves out a stay that has not started yet', () => {
		const route = buildRoute(day({ stays }), at(10));
		expect(route.stops.map((s) => s.name)).toEqual(['The boathouse', 'Corner bakery']);
	});

	it('draws the dotted line to a calendar entry ahead whose place is one of the stops', () => {
		const calendar = [
			event('late', 'Varnish the oars', 16, 17, 'The boathouse, by the slipway'),
			event('unplaced', 'Call the timber yard', 15, 16, null),
			event('gone', 'Morning swim', 8, 9, 'The boathouse'),
		];
		const route = buildRoute(day({ stays, calendar }), NOW.getTime());
		const planned = route.stops.filter((s) => s.planned);
		expect(planned.map((s) => s.name)).toEqual(['Varnish the oars']);
		expect(route.ahead.startsWith(`M${route.you?.x.toFixed(1)} ${route.you?.y.toFixed(1)} Q`)).toBe(true);
		expect(route.ahead.endsWith(`${planned[0].x.toFixed(1)} ${planned[0].y.toFixed(1)}`)).toBe(true);
		// During the entry you are at its place; after it the route says nothing.
		expect(route.at(at(16, 30))).toEqual({ x: planned[0].x, y: planned[0].y });
		expect(route.at(at(18))).toBeNull();
	});

	it('shows stops so far and entries not yet over as the clock\'s bars', () => {
		const calendar = [event('late', 'Varnish the oars', 16, 17, null), event('gone', 'Morning swim', 8, 9, null)];
		const bars = dayBars(day({ stays, calendar }), NOW.getTime());
		expect(bars.map((b) => [b.label, b.planned])).toEqual([
			['The boathouse', false],
			['Corner bakery', false],
			['The workshop', false],
			['Varnish the oars', true],
		]);
		expect(bars[2].to).toBe(NOW.getTime());
	});

	it('puts a thing on the clock only when it has a time of its own today', () => {
		const bounds = dayBounds(day({}));
		expect(itemTime({ task: task({ due_on: '2026-03-12', due_at: iso(17) }) }, bounds)).toBe(at(17));
		expect(itemTime({ task: task({ due_on: '2026-03-12' }) }, bounds)).toBeNull();
		expect(itemTime({ task: task({ due_on: '2026-03-13', due_at: new Date(2026, 2, 13, 9).toISOString() }) }, bounds)).toBeNull();
	});

	it('starts the clock at 6 AM, or earlier when the day does', () => {
		const bounds = dayBounds(day({}));
		expect(clockScale(bounds, []).fromHour).toBe(6);
		const early = clockScale(bounds, [at(4, 30)]);
		expect(early.fromHour).toBe(4);
		expect(early.x(at(4))).toBe(CLOCK_X0);
		expect(early.x(at(14))).toBeGreaterThan(early.x(at(9)));
		// A night at home that runs from the day's start leaves the clock at 6 AM.
		expect(clockScale(bounds, [at(0), at(9)]).fromHour).toBe(6);
	});

	it('moves a mark down a row when the one before is too close', () => {
		const placed = stack([
			{ key: 'a', x: 100 },
			{ key: 'b', x: 108 },
			{ key: 'c', x: 140 },
		]);
		expect(placed.a.row).toBe(0);
		expect(placed.b.row).toBe(1);
		expect(placed.c.row).toBe(0);
	});
	it('places a planned entry at one of your places looked up by name, and frames the map to reach it', () => {
		const mill = { name: 'The old mill', lat: 10.02, lng: 20.03 };
		const d = day({ stays, calendar: [event('e1', 'Collect the timber', 16, 17, 'The old mill, loading bay')] });
		expect(buildRoute(d, NOW.getTime()).stops.some((s) => s.planned)).toBe(false);
		const route = buildRoute(d, NOW.getTime(), undefined, [mill]);
		expect(route.stops.filter((s) => s.planned).map((s) => s.name)).toEqual(['Collect the timber']);
		expect(route.ahead).not.toBe('');
		expect(geoBox(d, NOW.getTime(), [mill])).toMatchObject({ lat1: 10.02, lng1: 20.03 });
		// A place nothing ahead names does not widen the map.
		expect(geoBox(d, NOW.getTime(), [{ name: 'The far barn', lat: 11, lng: 21 }])).toMatchObject({ lat1: 10.004, lng1: 20.012 });
	});

	it('keeps an entry already under way in the plan, by the time it began', () => {
		const d = day({ stays, calendar: [event('now', 'Glue the frame', 13, 15, 'The workshop'), event('done', 'Morning row', 7, 8, 'The boathouse')] });
		expect(entriesAhead(d, NOW.getTime()).map((e) => e.id)).toEqual(['now']);
		const planned = buildRoute(d, NOW.getTime()).stops.filter((s) => s.planned);
		expect(planned).toHaveLength(1);
		expect(planned[0].from).toBe(at(13));
	});
});

describe('placing names beside marks', () => {
	const bounds = { w: 400, h: 300 };
	const stop = (key: string, x: number, y: number, pinned = true) => ({ key, text: 'The boathouse', x, y, size: 12, right: true, pinned });

	it('keeps the names of marks that sit close together apart', () => {
		const marks = [stop('a', 100, 100), stop('b', 106, 108), stop('c', 112, 96)];
		const placed = fitLabels(marks, marks.map(markBox), bounds);
		const boxes = marks.map((m) => {
			const p = placed[m.key];
			expect(p).not.toBeNull();
			return { x1: p?.x ?? 0, y1: p?.y ?? 0, x2: (p?.x ?? 0) + labelWidth(m.text), y2: (p?.y ?? 0) + 20 };
		});
		for (let i = 0; i < boxes.length; i++) {
			for (let j = i + 1; j < boxes.length; j++) {
				const apart = boxes[i].x2 <= boxes[j].x1 || boxes[j].x2 <= boxes[i].x1 || boxes[i].y2 <= boxes[j].y1 || boxes[j].y2 <= boxes[i].y1;
				expect(apart).toBe(true);
			}
		}
	});

	it('turns a name to the other side at the edge of the drawing', () => {
		const placed = fitLabels([stop('a', 380, 100)], [], bounds).a;
		expect(placed).not.toBeNull();
		expect((placed?.x ?? 0) + labelWidth('The boathouse')).toBeLessThanOrEqual(380);
		// On the left it is drawn ending at its anchor, a fixed gap from the mark.
		expect(placed?.right).toBe(false);
		expect(placed?.anchor).toBe(380 - 6 - 5);
	});

	it('anchors a name on the right at its near edge', () => {
		const placed = fitLabels([stop('a', 100, 100)], [], bounds).a;
		expect(placed?.right).toBe(true);
		expect(placed?.anchor).toBe(100 + 6 + 5);
	});

	it('gives a name that sits away from its mark a line back to it', () => {
		// Everything beside the mark is taken, so the name moves further off.
		const wall = { x1: 0, y1: 77, x2: 400, y2: 123 };
		const placed = fitLabels([stop('a', 100, 100)], [wall], bounds).a;
		expect(placed?.lead).not.toBeNull();
	});

	it('leaves out a name that fits nowhere, unless it is pinned', () => {
		const full = [{ x1: 0, y1: 0, x2: 400, y2: 300 }];
		expect(fitLabels([stop('a', 100, 100, false)], full, bounds).a).toBeNull();
		expect(fitLabels([stop('a', 100, 100, true)], full, bounds).a).not.toBeNull();
	});
});

describe('the graph', () => {
	const threads = [
		{ id: 'bike', name: 'Fix the bike', group: 'The shed' },
		{ id: 'shelves', name: 'Build the shelves', group: 'The shed' },
		{ id: 'garden', name: 'Plan the garden', group: null },
	];
	const lay = layout(threads);
	const clash: Note = {
		key: 'clash:a:b',
		kind: 'clash',
		text: 'Bike shop pickup and Garden center visit overlap on Saturday.',
		subjects: ['/thread/bike', '/thread/garden'],
		about: 'Fix the bike',
		when: 'Saturday at 10:00 AM',
		action: 'See the day',
		action_route: '/day/2026-03-14',
		until: new Date(2026, 2, 14, 10).toISOString(),
	};
	const unplanned: Note = { ...clash, key: 'unplanned:chain', kind: 'unplanned', text: 'Order a new chain is due Friday with no time set aside.', subjects: ['/task/chain', '/thread/bike'] };
	const items = buildItems({
		tasks: [
			task({ id: 'chain', thread_id: 'bike', due_on: '2026-03-13' }),
			task({ id: 'trays', title: 'Buy seed trays', thread_id: 'garden' }),
			task({ id: 'stamps', title: 'Buy stamps' }),
		],
		notes: [clash, unplanned],
		attention: [],
		active: new Set(threads.map((t) => t.id)),
		now: NOW,
	});
	const graph = buildGraph({ items, threads, lay });
	const node = (id: string) => graph.nodes.find((n) => n.id === id);
	const linked = (from: string, to: string) => graph.links.find((l) => l.from === from && l.to === to);

	it('puts a hub for each thread on the radar\'s own angle, and hangs threads under a thing off that thing', () => {
		for (const t of threads) {
			const hub = node(hubId(t.id));
			const want = polar(lay.lanes[t.id].center, 150);
			expect(hub?.x).toBeCloseTo(want.x);
			expect(hub?.y).toBeCloseTo(want.y);
		}
		expect(linked('you', 'thing:The shed')?.style).toBe('tree');
		expect(linked('thing:The shed', hubId('bike'))?.rel).toBe('under');
		expect(linked('you', hubId('garden'))?.style).toBe('tree');
		expect(graph.nodes.filter((n) => n.kind === 'thing')).toHaveLength(1);
	});

	it('hangs a task off its thread, and a task in no thread off you', () => {
		expect(linked(hubId('garden'), 'task:trays')?.rel).toBe('in');
		expect(linked('you', 'task:stamps')?.style).toBe('loose');
	});

	it('sets an insight about two threads between their hubs, with a line to each', () => {
		const bridge = node('note:clash:a:b');
		expect(linked('note:clash:a:b', hubId('bike'))?.style).toBe('cross');
		expect(linked('note:clash:a:b', hubId('garden'))?.rel).toBe('about');
		expect(linked(hubId('bike'), 'note:clash:a:b')).toBeUndefined();
		// Nearer the center than either hub.
		expect(Math.hypot((bridge?.x ?? 0) - CX, (bridge?.y ?? 0) - CY)).toBeLessThan(150);
	});

	it('joins an insight about a task to that task', () => {
		expect(linked(hubId('bike'), 'note:unplanned:chain')?.rel).toBe('in');
		expect(linked('note:unplanned:chain', 'task:chain')?.rel).toBe('about');
	});

	it('gives a subject one node however many things link to it, and joins a thing to another thread\'s hub', () => {
		const nick = { key: 'person:nick', label: 'Nick' };
		const withLinks = buildGraph({
			items,
			threads,
			lay,
			extra: (it) =>
				it.key === 'task:trays'
					? [{ to: { subject: nick }, rel: 'for' }, { to: { thread: 'bike' }, rel: 'also about' }]
					: it.key === 'task:stamps'
						? [{ to: { subject: nick }, rel: 'for' }]
						: [],
		});
		expect(withLinks.nodes.filter((n) => n.kind === 'subject').map((n) => n.label)).toEqual(['Nick']);
		expect(withLinks.links.filter((l) => l.to === 'subject:person:nick')).toHaveLength(2);
		expect(withLinks.links.find((l) => l.from === 'task:trays' && l.to === hubId('bike'))?.rel).toBe('also about');
	});

	it('keeps every node clear of the others when a thread and the wedge are full', () => {
		const many = buildItems({
			tasks: [
				...Array.from({ length: 13 }, (_, i) => task({ id: `b${i}`, title: `Oil part ${i}`, thread_id: 'bike' })),
				...Array.from({ length: 24 }, (_, i) => task({ id: `l${i}`, title: `Sort drawer ${i}` })),
			],
			notes: [clash, { ...clash, key: 'clash:c:d' }],
			attention: [],
			active: new Set(threads.map((t) => t.id)),
			now: NOW,
		});
		const full = buildGraph({ items: many, threads, lay }).nodes;
		for (let i = 0; i < full.length; i++) {
			for (let j = i + 1; j < full.length; j++) {
				const gap = Math.hypot(full[i].x - full[j].x, full[i].y - full[j].y) - (full[i].size + full[j].size) / 2;
				expect(gap).toBeGreaterThan(1);
			}
		}
	});
});

describe('server questions', () => {
	const place = (over: Partial<PlaceQuestion> = {}): PlaceQuestion => ({
		ask: 'place',
		changed: false,
		id: 'pl_1',
		latitude: 29.95,
		longitude: -90.07,
		hours: 2.6,
		overnight: false,
		naming: { candidates: [], lodging: null },
		rejected_match: null,
		title: 'New Orleans',
		best_day: '2026-03-10',
		hours_a_week: 3,
		nights_a_week: 0,
		...over,
	});
	const cafe = { name: 'Corner Cup', meters: 22, category: 'dining' };
	const shop = { name: 'Reed Hardware', meters: 41, category: 'shopping' };

	it('asks about home and work with a yes or a no', () => {
		const home = placeQuestion(place({ ask: 'home', nights_a_week: 5.6 }));
		expect(home.text).toBe('Is this home?');
		expect(home.context).toBe('About 6 nights a week there');
		expect(home.replies.map((r) => r.answer.kind)).toEqual(['home', 'not_home']);
		expect(home.typed).toBeNull();
		const work = placeQuestion(place({ ask: 'work', changed: true, hours_a_week: 0.4 }));
		expect(work.text).toBe('Is this where you work now?');
		expect(work.context).toBe('Less than an hour a week there');
		expect(work.replies.map((r) => r.answer.kind)).toEqual(['work', 'not_work']);
	});

	it('names a place only when one named place is close, and never one you turned down', () => {
		const one = placeQuestion(place({ naming: { candidates: [cafe], lodging: null } }));
		expect(one.text).toBe('Were you at Corner Cup?');
		expect(one.replies[0].answer).toEqual({ kind: 'name', name: 'Corner Cup', category: 'dining' });
		expect(one.replies[1].answer).toEqual({ kind: 'not_this', name: 'Corner Cup' });
		const turnedDown = placeQuestion(place({ naming: { candidates: [cafe], lodging: null }, rejected_match: 'Corner Cup' }));
		expect(turnedDown.text).toBe('Where were you?');
		expect(turnedDown.typed).not.toBeNull();
		expect(turnedDown.replies.map((r) => r.label)).toEqual(['Just passing through']);
	});

	it('offers the close places to pick from when there are several', () => {
		const q = placeQuestion(place({ naming: { candidates: [cafe, shop], lodging: null } }));
		expect(q.text).toBe('Which of these were you at?');
		expect(q.replies.map((r) => r.label)).toEqual(['Corner Cup', 'Reed Hardware', 'Just passing through']);
		expect(q.typed).not.toBeNull();
	});

	it('asks where you stayed after a night, and never offers passing through', () => {
		const far = { name: 'Harbor Inn', meters: 180, category: 'lodging' };
		const open = placeQuestion(place({ overnight: true, naming: { candidates: [cafe], lodging: far } }));
		expect(open.text).toBe('Where did you stay that night?');
		expect(open.replies).toEqual([]);
		const close = placeQuestion(place({ overnight: true, naming: { candidates: [], lodging: { ...far, meters: 30 } } }));
		expect(close.text).toBe('Were you at Harbor Inn?');
	});

	it('lists a question as a thing that waits on you and is always shown', () => {
		const it = questionItem(placeQuestion(place({ ask: 'home' })));
		expect(it).toMatchObject({ key: 'ask:place:pl_1', kind: 'needs', noise: 0, title: 'Is this home?', meta: 'New Orleans' });
	});
});
