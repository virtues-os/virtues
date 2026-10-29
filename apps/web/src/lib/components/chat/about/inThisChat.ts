/**
 * What went into a conversation and what came out of it, read from the
 * transcript alone, for the chat's About page (ContextViewPanel). Nothing
 * here asks the server or a model, so the page can only name what the chat
 * itself touched, and a chat opened next week rebuilds the same list.
 */

import { buildCitationContextFromParts, getToolName, isToolCallPart, webKey } from '$lib/citations/builder';
import { getEntityTypeFromRoute, isRecordRoute, linkTarget, refIcon } from '$lib/utils/refRoutes';

export interface AsideItem {
	/** Route or URL; one row per thing however often it appears. */
	key: string;
	label: string;
	icon: string;
	/** A short second line: a kind, a site, what was done. */
	detail?: string;
	/** An app route, opened in the app. */
	route?: string;
	/** A web address, opened outside the app. */
	url?: string;
	/** Messages it appears in, in order, for "Show where". */
	messageIds: string[];
}

export interface InThisChat {
	mentioned: AsideItem[];
	/** Records and web pages the replies cite. */
	sources: AsideItem[];
	/** What the tools returned that no reply cites. */
	alsoRead: AsideItem[];
	broughtIn: AsideItem[];
	made: AsideItem[];
	/** The models that wrote the answers, one row per model, keyed `model:<id>`
	 *  with the provider as its detail. */
	writers: AsideItem[];
	/** Tool calls that reached outside: web search, browsing, an image model. */
	leftTheBox: AsideItem[];
}

interface Message {
	id?: string;
	role?: string;
	parts?: unknown[];
	model?: string;
	provider?: string;
}

type Part = Record<string, unknown> & { type?: string };

// `[label](target)` and the `![@label](/entity/id)` form the editor writes.
const LINK = /!?\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;

const SHOW_KIND: Record<string, string> = {
	chart: 'Chart',
	numbers: 'Numbers',
	table: 'Table',
	map: 'Map',
	timeline: 'Timeline',
};

const SHOW_ICON: Record<string, string> = {
	chart: 'mynaui:chart-bar',
	numbers: 'mynaui:hash',
	table: 'mynaui:table',
	map: 'mynaui:map',
	timeline: 'mynaui:calendar',
};

/** Rows in order of first appearance, merging repeats into one row. */
class Section {
	#rows = new Map<string, AsideItem>();

	add(item: Omit<AsideItem, 'messageIds'>, messageId: string): void {
		const row = this.#rows.get(item.key);
		if (!row) {
			this.#rows.set(item.key, { ...item, messageIds: messageId ? [messageId] : [] });
		} else if (messageId && !row.messageIds.includes(messageId)) {
			row.messageIds.push(messageId);
		}
	}

	has(key: string): boolean {
		return this.#rows.has(key);
	}

	get(key: string): AsideItem | undefined {
		return this.#rows.get(key);
	}

	get rows(): AsideItem[] {
		return [...this.#rows.values()];
	}
}

function textOf(parts: unknown[]): string {
	return parts
		.filter((p): p is Part => !!p && typeof p === 'object' && (p as Part).type === 'text')
		.map((p) => (typeof p.text === 'string' ? p.text : ''))
		.join('\n');
}

function linksIn(text: string): { label: string; url: string }[] {
	const out: { label: string; url: string }[] = [];
	for (const m of text.matchAll(LINK)) out.push({ label: m[1].replace(/^@/, '').trim(), url: m[2] });
	return out;
}

function hostOf(url: string): string {
	try {
		return new URL(url).host.replace(/^www\./, '');
	} catch {
		return url;
	}
}

/** A record's kind as a person would say it: `app_pages` reads "page". */
function kindOf(ontology: string): string {
	return ontology.replace(/^(app|data)_/, '').replace(/s$/, '').replace(/_/g, ' ');
}

/**
 * One key per thing. A reply cites a page as a record
 * (`/record/app_page/<id>`) while a mention and a tool that reads pages name
 * it by its route, so a page is keyed and opened as `/page/<id>`.
 */
function canonical(route: string): string {
	const page = /^\/record\/app_pages?\/([^/?#]+)$/.exec(route);
	return page ? `/page/${page[1]}` : route;
}

/** The row key a link in the transcript belongs to, so the two can light each other. */
export function keyForHref(href: string): string {
	return /^https?:\/\//i.test(href) ? webKey(href) : canonical(href);
}

function recordIcon(ontology: string | undefined): string {
	return ontology && kindOf(ontology) === 'page' ? refIcon('page') : 'mynaui:database';
}

function str(v: unknown): string | undefined {
	return typeof v === 'string' && v.trim() ? v : undefined;
}

/** An entity ref as a row: the name as the text wrote it, the type's icon. */
function entityRow(url: string, label: string): Omit<AsideItem, 'messageIds'> {
	const type = getEntityTypeFromRoute(url);
	return { key: url, label: label || url, icon: refIcon(type), route: url };
}

export function inThisChat(messages: readonly unknown[]): InThisChat {
	const mentioned = new Section();
	const sources = new Section();
	const read = new Section();
	const broughtIn = new Section();
	const made = new Section();
	const writers = new Section();
	const left = new Section();

	for (const raw of messages) {
		const message = raw as Message;
		const id = message.id ?? '';
		const parts = Array.isArray(message.parts) ? message.parts : [];
		const links = linksIn(textOf(parts));

		if (message.role === 'user') {
			for (const p of parts as Part[]) {
				if (p?.type !== 'file') continue;
				const name = str(p.filename) ?? 'Attached file';
				const url = str(p.url);
				broughtIn.add(
					{ key: url ?? `${id}:${name}`, label: name, icon: refIcon('file', { mimeType: str(p.mediaType), filename: name }), detail: 'Attached', url },
					id
				);
			}
			for (const { label, url } of links) {
				const target = linkTarget(url);
				if (target.kind === 'entity') broughtIn.add({ ...entityRow(url, label), detail: 'You mentioned' }, id);
				else if (target.kind === 'external') broughtIn.add({ key: webKey(url), label: label || hostOf(url), icon: 'mynaui:globe', detail: hostOf(url), url }, id);
			}
			continue;
		}

		if (message.role !== 'assistant') continue;

		// The answer itself went to whichever model wrote it.
		const model = str(message.model);
		if (model) {
			const provider = str(message.provider);
			writers.add({ key: `model:${model}`, label: model, icon: 'mynaui:sparkles', detail: provider }, id);
		}

		// What this turn's tools returned, so its citations can be told apart
		// from what it only read.
		const context = buildCitationContextFromParts(parts);

		for (const { label, url } of links) {
			const target = linkTarget(url);
			if (target.kind === 'entity') {
				if (isRecordRoute(url)) {
					const record = context.byRoute.get(url);
					const key = canonical(url);
					const page = key !== url;
					sources.add(
						{
							key,
							label: record?.title ?? label,
							icon: page ? refIcon('page') : recordIcon(record?.ontology),
							detail: page ? 'Page' : record ? kindOf(record.ontology) : undefined,
							route: key,
						},
						id
					);
				} else {
					mentioned.add(entityRow(url, label), id);
				}
			} else if (target.kind === 'query') {
				// The rows behind a figure or a claim, which open in a tab of their own.
				sources.add({ key: url, label: label || 'Query', icon: 'mynaui:table', detail: 'Query of your records', route: url }, id);
			} else if (target.kind === 'external') {
				const page = context.byUrl.get(webKey(url));
				sources.add({ key: webKey(url), label: page?.title ?? label ?? hostOf(url), icon: 'mynaui:globe', detail: hostOf(url), url }, id);
			}
		}

		for (const record of context.byRoute.values()) {
			const key = canonical(record.route);
			const page = key !== record.route;
			read.add(
				{
					key,
					label: record.title ?? kindOf(record.ontology),
					icon: page ? refIcon('page') : recordIcon(record.ontology),
					detail: page ? 'Page' : kindOf(record.ontology),
					route: key,
				},
				id
			);
		}
		for (const [key, page] of context.byUrl) {
			read.add({ key, label: page.title, icon: 'mynaui:globe', detail: hostOf(page.url), url: page.url }, id);
		}

		for (const p of parts) {
			if (!isToolCallPart(p)) continue;
			const name = getToolName(p);
			const input = (p.input ?? {}) as Record<string, unknown>;
			const done = p.state === 'output-available';
			const out = (done && p.output && typeof p.output === 'object' ? p.output : {}) as Record<string, unknown>;

			switch (name) {
				// Tools that read one thing whole: a page, or an item in a project.
				case 'get_page_content': {
					const pageId = str(out.page_id) ?? str(input.page_id);
					if (pageId) read.add({ key: `/page/${pageId}`, label: str(out.title) ?? 'Page', icon: refIcon('page'), detail: 'Page', route: `/page/${pageId}` }, id);
					break;
				}
				case 'get_project_item': {
					// The item comes back named by its kind: a chat or page by
					// `title`, a project by `name`.
					const route = str(input.item_url);
					const kind = str(out.type);
					if (route?.startsWith('/')) {
						read.add(
							{ ...entityRow(route, str(out.title) ?? str(out.name) ?? (kind ? `Untitled ${kind}` : 'Untitled')), detail: kind ? kind[0].toUpperCase() + kind.slice(1) : undefined },
							id
						);
					}
					break;
				}
				case 'web_search': {
					const query = str(input.query);
					if (query) left.add({ key: `search:${query}`, label: query, icon: 'mynaui:search', detail: 'Web search' }, id);
					break;
				}
				case 'browse': {
					const task = str(input.task);
					if (task) left.add({ key: `browse:${p.toolCallId}`, label: task, icon: 'mynaui:globe', detail: 'Browsed the web' }, id);
					break;
				}
				case 'generate_image': {
					const prompt = str(out.prompt) ?? str(input.prompt) ?? 'Image';
					if (done && str(out.url)) {
						made.add({ key: str(out.url)!, label: prompt, icon: 'mynaui:image', detail: 'Image', url: str(out.url) }, id);
					}
					left.add({ key: `image:${p.toolCallId}`, label: prompt, icon: 'mynaui:image', detail: 'Sent to an image model' }, id);
					break;
				}
				case 'create_page': {
					const pageId = str(out.page_id);
					if (pageId) made.add({ key: `/page/${pageId}`, label: str(out.title) ?? 'Untitled page', icon: refIcon('page'), detail: 'New page', route: `/page/${pageId}` }, id);
					break;
				}
				case 'edit_page': {
					const edit = out.edit as Record<string, unknown> | undefined;
					const pageId = str(edit?.page_id);
					// A new page edited in the same chat is already listed as new.
					if (out.applied && pageId && !made.has(`/page/${pageId}`)) {
						made.add({ key: `/page/${pageId}`, label: str(out.page_title) ?? 'Page', icon: refIcon('page'), detail: 'Edited', route: `/page/${pageId}` }, id);
					}
					break;
				}
				case 'show': {
					const kind = str(out.kind) ?? str(input.kind);
					if (!kind || kind === 'choices' || p.state === 'output-error') break;
					made.add(
						{ key: `show:${p.toolCallId}`, label: str(out.title) ?? str(input.title) ?? SHOW_KIND[kind] ?? 'Figure', icon: SHOW_ICON[kind] ?? 'mynaui:chart-bar', detail: SHOW_KIND[kind] },
						id
					);
					break;
				}
				case 'applet':
				case 'setup_applet': {
					if (name === 'applet' && input.action !== 'setup') break;
					const appletId = str(out.applet_id);
					if (appletId && out.status !== 'check_failed') {
						made.add({ key: `applet:${appletId}`, label: str(out.name) ?? 'Applet', icon: 'mynaui:sparkles', detail: out.status === 'updated' ? 'Applet changed' : 'Applet' }, id);
					}
					break;
				}
				case 'create_task':
				case 'update_task': {
					if (!done) break;
					const tasks = (Array.isArray(out.tasks) ? out.tasks : out.task ? [out.task] : []) as Record<string, unknown>[];
					for (const task of tasks) {
						const title = str(task?.title);
						if (!title) continue;
						made.add({ key: `task:${str(task.id) ?? title}`, label: title, icon: 'mynaui:check-square', detail: name === 'create_task' ? 'Task added' : 'Task updated' }, id);
					}
					break;
				}
			}
		}
	}

	// "Also read" is what the chat read and names nowhere else: a record a
	// reply cites is a source, and a thing you brought in or the replies
	// mention is already listed once.
	const alsoRead = read.rows.filter((r) => ![sources, mentioned, broughtIn, made].some((s) => s.has(r.key)));

	// A citation's link text is often a word ("page"); the title a tool read
	// names it better.
	const cited = sources.rows.map((r) => {
		const title = read.get(r.key)?.label;
		return title && title !== 'Page' ? { ...r, label: title } : r;
	});

	return {
		mentioned: mentioned.rows,
		sources: cited,
		alsoRead,
		broughtIn: broughtIn.rows,
		made: made.rows,
		writers: writers.rows,
		leftTheBox: left.rows,
	};
}
