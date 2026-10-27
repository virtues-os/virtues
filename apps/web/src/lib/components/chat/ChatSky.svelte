<!--
	ChatSky.svelte

	What an empty new chat shows above the composer when there is a day to
	draw: a dot for each part of it (`GET /api/sky/day`), placed left to right
	by time. A dot's size is how long it lasted, and a place visited twice
	keeps one height so going back to it lines up. Rings are the next calendar
	items. Hovering a dot names it; the short dotted mark is now, and its
	hover carries the weather and what is next.

	Before anything is recorded today, or with the sky switched off in
	Settings › Assistant, it shows the mark and the word instead.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { fade } from 'svelte/transition';
	import NewChatMark from './NewChatMark.svelte';
	import { getAssistantProfile, getSkyDay, type SkyDay, type SkyMoment } from '$lib/api/client';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';

	const HEIGHT = 140;
	const PAD = 12;
	/** The day the arrival last played, so it plays once a day and not on
	 *  every new chat. */
	const ARRIVED_KEY = 'virtues.chatSky.arrived';

	let day = $state<SkyDay | null>(null);
	let ready = $state(false);
	let width = $state(640);
	let hover = $state<string | null>(null);
	let arrive = $state(false);

	onMount(() => {
		const date = getLocalDateSlug();
		Promise.all([
			// Unreadable preferences leave the sky on, its default.
			getAssistantProfile<{ ui_preferences?: Record<string, unknown> }>().catch(() => null),
			getSkyDay(date)
		])
			.then(([profile, sky]) => {
				if (profile?.ui_preferences?.chat_sky === false) return;
				day = sky;
				try {
					arrive = localStorage.getItem(ARRIVED_KEY) !== date;
					localStorage.setItem(ARRIVED_KEY, date);
				} catch {
					arrive = false;
				}
			})
			// A server without the sky, or one that cannot read the day, shows the mark.
			.catch(() => (day = null))
			.finally(() => (ready = true));
	});

	function hash(s: string): number {
		let h = 2166136261;
		for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
		h = Math.imul(h ^ (h >>> 16), 0x85ebca6b);
		h = Math.imul(h ^ (h >>> 13), 0xc2b2ae35);
		h ^= h >>> 16;
		return (h >>> 0) / 4294967295;
	}

	function clock(hours: number, suffix = true): string {
		const h = ((Math.floor(hours) % 24) + 24) % 24;
		const m = Math.round((hours - Math.floor(hours)) * 60);
		const hh = h + (m === 60 ? 1 : 0);
		const mm = m === 60 ? 0 : m;
		const text = `${((hh % 24) + 11) % 12 + 1}:${String(mm).padStart(2, '0')}`;
		return suffix ? `${text} ${hh % 24 < 12 ? 'AM' : 'PM'}` : text;
	}

	function span(start: number, end: number): string {
		const sameHalf = (((Math.floor(start) % 24) + 24) % 24 < 12) === (((Math.floor(end) % 24) + 24) % 24 < 12);
		return `${clock(start, !sameHalf)}–${clock(end)}`;
	}

	function when(m: SkyMoment, now: number | null): string {
		if (m.kind === 'sleep') return `Asleep ${clock(m.start_hour)}, up at ${clock(m.end_hour)}`;
		if (m.planned) return `${span(m.start_hour, m.end_hour)} · on your calendar`;
		if (now !== null && m.start_hour <= now && now < m.end_hour) return `Since ${clock(m.start_hour)}`;
		return span(m.start_hour, m.end_hour);
	}

	type Dot = {
		m: SkyMoment;
		x: number;
		y: number;
		w: number;
		h: number;
		tone: string;
		ring: boolean;
		delay: number;
		breathe: number;
		when: string;
	};

	const frame = $derived.by(() => {
		const moments = day?.moments ?? [];
		const awake = moments.filter((m) => m.kind !== 'sleep');
		const from = Math.min(5.5, ...awake.map((m) => m.start_hour - 0.4));
		const to = Math.max(24.5, ...moments.map((m) => m.end_hour + 0.3), (day?.now_hour ?? 0) + 0.3);
		const x = (h: number) => PAD + ((Math.min(Math.max(h, from), to) - from) / (to - from)) * (width - 2 * PAD);
		return { from, to, x };
	});

	const dots = $derived.by((): Dot[] => {
		const moments = day?.moments ?? [];
		const now = day?.now_hour ?? null;
		// A place visited more than once today keeps one height; one-off stops float.
		const visits = new Map<string, number>();
		for (const m of moments) if (m.place_id) visits.set(m.place_id, (visits.get(m.place_id) ?? 0) + 1);
		const fixed = [...visits].filter(([, n]) => n > 1).map(([p]) => p).sort((a, b) => hash(a) - hash(b));
		const level = new Map(fixed.map((p, i) => [p, 28 + (i + 0.3 + hash(p + 'g') * 0.4) * (84 / fixed.length)]));
		// Floating dots step through the height by the golden ratio from a
		// seed for the day: it reads as scattered, a few dots can never bunch
		// into one band the way independent hashes can, and a dot added later
		// in the day leaves the earlier ones where they were.
		const seed = hash(day?.date ?? '');
		let floating = 0;
		const floatY = new Map<string, number>();
		for (const m of moments) {
			if (m.place_id && level.has(m.place_id)) continue;
			floatY.set(m.id, 22 + ((seed + floating++ * 0.618034) % 1) * 96);
		}
		return moments.map((m, i) => {
			const minutes = Math.max(0, (m.end_hour - m.start_hour) * 60);
			const size = Math.round((2 * Math.min(4.6, 1.3 + 2.4 * Math.sqrt(Math.min(minutes, 240) / 240)) + 1) * 10) / 10;
			const sleep = m.kind === 'sleep';
			const mid = sleep ? frame.from + 0.45 : (m.start_hour + m.end_hour) / 2;
			const base = (m.place_id ? level.get(m.place_id) : undefined) ?? floatY.get(m.id) ?? 70;
			const ongoing = now !== null && !sleep && !m.planned && m.start_hour <= now && now < m.end_hour;
			return {
				m,
				x: Math.round(frame.x(mid) * 10) / 10,
				y: Math.round(Math.min(124, Math.max(16, base + (hash(m.id + 'j') - 0.5) * 6)) * 10) / 10,
				w: sleep ? 18 : size,
				h: sleep ? 7 : size,
				// Until the day is scored, the kind sets the ink: what you did or
				// planned reads a little darker than where you were.
				tone: m.kind === 'workout' ? '55%' : m.kind === 'event' ? '50%' : sleep ? '42%' : '30%',
				ring: m.planned || ongoing,
				delay: Math.min(1300, i * 70),
				breathe: Math.round((6 + hash(m.id + 'tw') * 7) * 10) / 10,
				when: when(m, now)
			};
		});
	});

	const next = $derived(day?.moments.find((m) => m.planned) ?? null);

	const weatherLine = $derived.by(() => {
		const w = day?.weather;
		if (!w) return '';
		const temp = w.temperature_c === null ? '' : `${Math.round((w.temperature_c * 9) / 5 + 32)}°, `;
		const condition = w.condition ? w.condition.charAt(0).toLowerCase() + w.condition.slice(1) : '';
		return `${temp}${condition}`.replace(/, $/, '');
	});

	const nowX = $derived(day?.now_hour == null ? null : Math.round(frame.x(day.now_hour) * 10) / 10);

	const tip = $derived.by(() => {
		if (!hover) return null;
		const clampX = (x: number) => Math.min(width - 90, Math.max(90, x));
		if (hover === 'now' && nowX !== null && day?.now_hour != null) {
			const title = weatherLine ? `${clock(day.now_hour)} · ${weatherLine}` : clock(day.now_hour);
			const why = next ? `Next, ${next.label} at ${clock(next.start_hour)}` : 'Nothing else on your calendar today';
			return { x: clampX(nowX), y: -46, title, why };
		}
		const d = dots.find((d) => d.m.id === hover);
		if (!d) return null;
		return { x: clampX(d.x), y: d.y > HEIGHT / 2 ? d.y - 56 : d.y + 16, title: d.m.label, why: d.when };
	});

	// A mouse names a dot while it rests on it; a finger, which also fires
	// enter, toggles the name with each tap instead.
	function enter(e: PointerEvent, id: string) {
		if (e.pointerType === 'mouse') hover = id;
	}
	function leave(e: PointerEvent) {
		if (e.pointerType === 'mouse') hover = null;
	}
	function touch(e: PointerEvent, id: string) {
		if (e.pointerType !== 'mouse') hover = hover === id ? null : id;
	}
</script>

{#if ready && day && day.moments.length > 0}
	<div
		class="sky"
		class:arrive
		bind:clientWidth={width}
		role="group"
		aria-label="Your day so far"
		in:fade={{ duration: 200 }}
	>
		{#if nowX !== null}
			<!-- svelte-ignore a11y_no_static_element_interactions -- hover only adds what the list below already says to screen readers -->
			<span
				class="now"
				style="left: {nowX}px"
				aria-hidden="true"
				onpointerenter={(e) => enter(e, 'now')}
				onpointerleave={leave}
				onpointerdown={(e) => touch(e, 'now')}
			></span>
		{/if}
		{#each dots as d (d.m.id)}
			<!-- svelte-ignore a11y_no_static_element_interactions -- hover only adds what the list below already says to screen readers -->
			<span
				class="dot"
				style="left: {d.x}px; top: {d.y}px; --d: {d.delay}ms; --tw: {d.breathe}s"
				aria-hidden="true"
				onpointerenter={(e) => enter(e, d.m.id)}
				onpointerleave={leave}
				onpointerdown={(e) => touch(e, d.m.id)}
			>
				<span class="ink" class:ring={d.ring} style="width: {d.w}px; height: {d.h}px; --tone: {d.tone}"></span>
			</span>
		{/each}
		{#if tip}
			<div class="tip" style="left: {tip.x}px; top: {tip.y}px" aria-hidden="true">
				<div>{tip.title}</div>
				<div class="tip-why">{tip.why}</div>
			</div>
		{/if}
		<ul class="sr-only">
			{#each dots as d (d.m.id)}
				<li>{d.m.label}, {d.when}</li>
			{/each}
		</ul>
	</div>
{:else if ready}
	<NewChatMark />
{/if}

<style>
	.sky {
		position: relative;
		width: min(640px, calc(100% - 32px));
		height: 140px;
		pointer-events: auto;
		color: var(--color-foreground);
	}

	.dot {
		position: absolute;
		width: 26px;
		height: 26px;
		margin: -13px 0 0 -13px;
		display: grid;
		place-items: center;
		border-radius: var(--radius-full);
		cursor: default;
	}

	.ink {
		display: block;
		box-sizing: border-box;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, currentColor var(--tone), transparent);
		transition: transform 220ms var(--ease-premium);
	}

	.ink.ring {
		background: transparent;
		border: 1px solid color-mix(in srgb, currentColor 36%, transparent);
	}

	.dot:hover .ink {
		transform: scale(1.4);
	}

	.now {
		position: absolute;
		top: 41px;
		width: 16px;
		height: 58px;
		margin-left: -8px;
		display: flex;
		justify-content: center;
		cursor: default;
	}

	.now::before {
		content: '';
		width: 1px;
		height: 100%;
		background-image: repeating-linear-gradient(
			to bottom,
			color-mix(in srgb, currentColor 34%, transparent) 0 1px,
			transparent 1px 4px
		);
	}

	.tip {
		position: absolute;
		transform: translateX(-50%);
		pointer-events: none;
		white-space: nowrap;
		text-align: center;
		font-size: 13px;
		line-height: 18px;
		padding: 4px 10px;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--color-background) 94%, transparent);
		color: var(--color-foreground);
		z-index: 1;
	}

	.tip-why {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}

	@media (prefers-reduced-motion: no-preference) {
		/* The day arriving in the order it happened, on the first new chat of
		   the day; afterwards each dot breathes on its own clock. */
		.arrive .ink {
			animation:
				sky-arrive 0.7s cubic-bezier(0.22, 1, 0.36, 1) var(--d) backwards,
				sky-breathe var(--tw) ease-in-out calc(var(--d) + 0.7s) infinite;
		}
		.sky:not(.arrive) .ink {
			animation: sky-breathe var(--tw) ease-in-out infinite;
		}
		.tip {
			animation: sky-tip 160ms ease-out;
		}
	}

	@keyframes sky-arrive {
		from {
			opacity: 0;
			transform: scale(0.2) translateY(4px);
		}
	}
	/* Quick to dim and slow to return, so it breathes rather than blinks. */
	@keyframes sky-breathe {
		0%,
		100% {
			opacity: 1;
		}
		18% {
			opacity: 0.6;
		}
	}
	@keyframes sky-tip {
		from {
			opacity: 0;
		}
	}
</style>
