/**
 * What home's tiles say beyond a name: a line from inside the thing, and the
 * week's nights for the sleep tile. Each read can fail or find nothing, and
 * then its tile shows less rather than something made up.
 */
import { getChat, getPage, getSkyWeek, type SkyDay } from '$lib/api/client';
import { deduplicateMessages, toUiMessage } from '$lib/components/chat/state/transcript';
import { weekMonday } from '$lib/components/chat/sky';
import { projectStore } from '$lib/stores/project.svelte';
import { getLocalDateSlug, parseDateSlug } from '$lib/utils/dateUtils';
import { markdownWords } from '$lib/wiki/inlineMarkdown';

/** The first sentence of some markdown that isn't just its title again. */
export function firstSentence(md: string, title?: string): string | null {
	const lines = markdownWords(md)
		.split('\n')
		.map((l) => l.trim())
		.filter((l) => l && l !== title?.trim());
	const first = lines[0];
	if (!first) return null;
	return first.split(/(?<=[.!?])\s+/)[0] ?? first;
}

/** The first sentence of a chat's last reply. */
export async function lastReplyLine(chatId: string): Promise<string | null> {
	const data = await getChat<{ messages?: unknown[] }>(chatId);
	const turns = deduplicateMessages(data.messages ?? []).map((m) => toUiMessage(m, new Map())) as {
		role: string;
		parts: { type?: string; text?: string }[];
	}[];
	const reply = turns.findLast((t) => t.role === 'assistant');
	const text = reply?.parts
		.filter((p) => p.type === 'text' && p.text)
		.map((p) => p.text)
		.join('\n\n');
	return text ? firstSentence(text) : null;
}

/** A line from inside a pinned thing, for the kinds that have one. */
export async function pinLine(url: string, title: string): Promise<string | null> {
	const [, kind, id] = url.split('?')[0].match(/^\/(page|chat|thread)\/([^/]+)$/) ?? [];
	if (kind === 'page') {
		const page = await getPage(id);
		return firstSentence(page.content ?? '', page.title ?? title);
	}
	if (kind === 'chat') return lastReplyLine(id);
	if (kind === 'thread') {
		const brief = projectStore.byId(id)?.instructions;
		return brief ? firstSentence(brief, title) : null;
	}
	return null;
}

/** A night as the sky reports it: the day it woke into, and minutes asleep. */
export interface Night {
	date: string;
	minutes: number | null;
}

export interface Nights {
	/** The last seven days ending today, oldest first. */
	week: Night[];
	/** Last night, when your server has it. */
	last: number | null;
	/** Last night's sleep record, when it has one. */
	lastUrl: string | null;
	/** How last night sits against the other nights of the week, once there
	 *  are three to compare with. */
	usual: 'about' | 'longer' | 'shorter' | null;
}

/** Minutes asleep from the sky's sleep label (`api::sky`, "Sleep, 7 h 12 m"). */
function asleepMinutes(day: SkyDay | undefined): number | null {
	const label = day?.moments.find((m) => m.kind === 'sleep')?.label;
	const m = label?.match(/(\d+) h (\d+) m/);
	return m ? Number(m[1]) * 60 + Number(m[2]) : null;
}

/** Within this many minutes of the median night, last night was usual. */
const USUAL_MINUTES = 30;
const FRESH_MS = 5 * 60_000;
let cached: { date: string; at: number; nights: Promise<Nights> } | null = null;

async function readNights(today: string): Promise<Nights> {
	const ago = parseDateSlug(today);
	ago.setDate(ago.getDate() - 6);
	const from = getLocalDateSlug(ago);
	const weeks = await Promise.all(
		[...new Set([weekMonday(from), weekMonday(today)])].map((monday) => getSkyWeek(monday))
	);
	const byDate = new Map(weeks.flatMap((w) => w.days).map((d) => [d.date, d]));
	const week: Night[] = [];
	for (let i = 6; i >= 0; i--) {
		const d = parseDateSlug(today);
		d.setDate(d.getDate() - i);
		const date = getLocalDateSlug(d);
		week.push({ date, minutes: asleepMinutes(byDate.get(date)) });
	}
	const last = week.at(-1)?.minutes ?? null;
	const lastUrl = byDate.get(today)?.moments.find((m) => m.kind === 'sleep')?.url ?? null;
	const others = week
		.slice(0, -1)
		.map((n) => n.minutes)
		.filter((m): m is number => m !== null)
		.sort((a, b) => a - b);
	let usual: Nights['usual'] = null;
	if (last !== null && others.length >= 3) {
		const median = others[Math.floor(others.length / 2)];
		usual = Math.abs(last - median) <= USUAL_MINUTES ? 'about' : last > median ? 'longer' : 'shorter';
	}
	return { week, last, lastUrl, usual };
}

/** The week's nights. Home is opened often, so one read serves a few minutes. */
export function recentNights(): Promise<Nights> {
	const date = getLocalDateSlug();
	if (cached && cached.date === date && Date.now() - cached.at < FRESH_MS) return cached.nights;
	const nights = readNights(date);
	cached = { date, at: Date.now(), nights };
	nights.catch(() => {
		if (cached?.nights === nights) cached = null;
	});
	return nights;
}

/** "7 h 12 m", the way the sky writes it. */
export function hoursMinutes(minutes: number): string {
	return `${Math.floor(minutes / 60)} h ${String(minutes % 60).padStart(2, '0')} m`;
}
