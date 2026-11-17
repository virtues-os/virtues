import { describe, expect, it } from 'vitest';
import type { SkyContext, SkyDay, SkyMoment, SkyWeek } from '$lib/api/client';
import { SKY_PAD, layoutContext, layoutDay, layoutMoment, layoutWeek, momentMatches, span } from './sky';

const lunch: SkyMoment = {
	id: 'event:c1',
	kind: 'event',
	start_hour: 12,
	end_hour: 13,
	label: 'Lunch with David Okafor, Café Rosa',
	place_id: 'pl_rosa',
	planned: false,
	url: '/record/calendar_event/c1',
	links: [{ url: '/record/financial_transaction/t1', label: '$42.10 at Bakery', kind: 'purchase' }],
	novelty: null
};

describe('momentMatches', () => {
	it('matches the label or a link, ignoring case', () => {
		expect(momentMatches(lunch, 'LUN')).toBe(true);
		expect(momentMatches(lunch, 'bakery')).toBe(true);
		expect(momentMatches(lunch, 'dinner')).toBe(false);
	});

	it('matches nothing for an empty query', () => {
		expect(momentMatches(lunch, '  ')).toBe(false);
	});
});

describe('span', () => {
	it('names AM or PM once when both ends share it', () => {
		expect(span(12, 13)).toBe('12:00–1:00 PM');
		expect(span(9.5, 13.25)).toBe('9:30 AM–1:15 PM');
		expect(span(-1.0833, 6.5833)).toBe('10:55 PM–6:35 AM');
	});
});

const moment = (id: string, kind: SkyMoment['kind'], start: number, end: number, extra: Partial<SkyMoment> = {}): SkyMoment => ({
	id,
	kind,
	start_hour: start,
	end_hour: end,
	label: id,
	place_id: null,
	planned: false,
	url: `/record/x/${id}`,
	links: [],
	novelty: null,
	...extra
});

const invented = (date: string, moments: SkyMoment[], now: number | null): SkyDay => ({
	date,
	zone: 'UTC',
	now_hour: now,
	moments,
	weather: null
});

describe('layoutDay', () => {
	const day = invented(
		'2031-03-13',
		[
			moment('night', 'sleep', -1, 6.5),
			moment('studio', 'stay', 8.5, 11.6),
			moment('lunch', 'event', 12, 13, { planned: true }),
			moment('dinner', 'event', 18.5, 20, { planned: true })
		],
		11.7
	);

	it('gives the hours so far most of the width and folds the rest after now', () => {
		const { stars, now } = layoutDay(day, 640);
		expect(now).not.toBeNull();
		expect(now!.x).toBeGreaterThan(640 * 0.5);
		const dinner = stars.find((s) => s.key === 'dinner')!;
		expect(dinner.x).toBeGreaterThan(now!.x);
		expect(dinner.x).toBeLessThanOrEqual(640 - SKY_PAD);
	});

	it('puts the unusual above the routine and gives it the accent', () => {
		const scored = invented(
			'2031-03-13',
			[
				moment('desk', 'stay', 9, 11, { place_id: 'pl_studio', novelty: 0.05 }),
				moment('cafe', 'stay', 13, 14, { place_id: 'pl_new', novelty: 1 })
			],
			null
		);
		const stars = Object.fromEntries(layoutDay(scored, 640).stars.map((s) => [s.key, s]));
		expect(stars.cafe.y).toBeLessThan(stars.desk.y - 40);
		expect([stars.desk.accent, stars.cafe.accent]).toEqual([false, true]);
	});

	it('draws a longer night as a longer band', () => {
		const w = (hours: number) =>
			layoutDay(invented('2031-03-13', [moment('night', 'sleep', 6.5 - hours, 6.5), moment('desk', 'stay', 9, 11)], null), 640).stars[0].w;
		expect(w(8)).toBeGreaterThan(w(5) + 20);
	});

	it('draws what is going on, what is planned and the night each its own way', () => {
		const looks = Object.fromEntries(layoutDay(day, 640).stars.map((s) => [s.key, s.look]));
		expect(looks).toEqual({ night: 'night', studio: 'ongoing', lunch: 'planned', dinner: 'planned' });
	});
});

describe('layoutWeek', () => {
	const week: SkyWeek = {
		start: '2031-03-10',
		threads: [{ url: '/project/p', label: 'Friday review', kind: 'project', moments: ['2031-03-10|a', '2031-03-12|c'] }],
		days: Array.from({ length: 7 }, (_, i) =>
			invented(
				`2031-03-${10 + i}`,
				i === 0
					? [moment('n0', 'sleep', -1, 7), moment('a', 'event', 9, 10), moment('b', 'stay', 12, 13)]
					: i === 2
						? [moment('c', 'event', 15, 16)]
						: [],
				null
			)
		)
	};

	it('gathers a thread on its own line and everything else on the bottom one', () => {
		const { stars, letters, labels } = layoutWeek(week, 640, '2031-03-12');
		expect(letters.map((l) => l.today)).toEqual([false, false, true, false, false, false, false]);
		const at = Object.fromEntries(stars.filter((s) => !s.hidden).map((s) => [s.key, s.y]));
		expect(at).toEqual({ '2031-03-10|0': 24, '2031-03-10|other': 250, '2031-03-12|0': 24 });
		expect(labels.map((l) => [l.thread.label, l.align])).toEqual([['Friday review', 'right']]);
	});

	it("folds the shown day's own dots into its week dots, undrawn", () => {
		const { stars } = layoutWeek(week, 640, '2031-03-12', '2031-03-10');
		const a = stars.find((s) => s.key === 'a')!;
		const group = stars.find((s) => s.key === '2031-03-10|0')!;
		expect([a.hidden, a.x, a.y]).toEqual([true, group.x, group.y]);
	});

	it('draws a thread as one storyline across the days it touches', () => {
		const { paths } = layoutWeek(week, 640, '2031-03-12');
		expect(paths).toHaveLength(1);
		expect(paths[0].url).toBe('/project/p');
	});
});

describe('layoutContext', () => {
	it('places the labels in one row without overlap, each joined to its moments', () => {
		const day = invented('2031-03-13', [moment('a', 'stay', 9, 10), moment('b', 'event', 9.5, 10.5), moment('c', 'stay', 15, 16)], null);
		const context: SkyContext = {
			date: day.date,
			threads: [
				{ url: '/project/p', label: 'Friday review', kind: 'project', moments: ['a', 'b'] },
				{ url: '/chat/c', label: 'Menu ideas for the week', kind: 'chat', moments: ['a'] },
				{ url: '/page/g', label: 'Garden', kind: 'page', moments: ['c'] }
			]
		};
		const { labels, paths } = layoutContext(day, context, 640);
		const joins = (url: string) => paths.filter((p) => p.url === url).length;
		expect([joins('/project/p'), joins('/chat/c'), joins('/page/g')]).toEqual([2, 1, 1]);
		for (const a of labels)
			for (const b of labels)
				if (a !== b && a.y === b.y) expect(Math.abs(a.x - b.x)).toBeGreaterThan(60);
	});
});

describe('layoutMoment', () => {
	it('writes the pieces under the line without running them into each other', () => {
		const lunch: SkyMoment = { ...moment('event:c1', 'stay', 9, 12), label: 'Studio North' };
		const detail = {
			date: '2031-03-13',
			now_hour: null,
			moment: lunch,
			pieces: [
				{ id: 'p1', kind: 'arrive' as const, hour: 9, label: 'Arrived at Studio North', detail: null, url: null },
				{ id: 'p2', kind: 'message' as const, hour: 10.25, label: 'Can you send the slides before lunch?', detail: 'Nick', url: '/record/m/1' },
				{ id: 'p3', kind: 'message' as const, hour: 10.35, label: 'Sending them now.', detail: 'You', url: '/record/m/2' },
				{ id: 'p4', kind: 'voice' as const, hour: 10.67, label: 'Room for Friday', detail: null, url: '/record/v/1' }
			]
		};
		const { pieceLabels } = layoutMoment(detail, null, 640);
		const span = (l: (typeof pieceLabels)[number]) => [l.x - (l.text.length * 6.4 + 8) / 2, l.x + (l.text.length * 6.4 + 8) / 2];
		for (const a of pieceLabels) {
			const [a0, a1] = span(a);
			expect(a0).toBeGreaterThanOrEqual(0);
			expect(a1).toBeLessThanOrEqual(640);
			for (const b of pieceLabels) {
				if (a === b || a.y !== b.y) continue;
				const [b0, b1] = span(b);
				expect(a1 <= b0 || b1 <= a0).toBe(true);
			}
		}
	});
});
