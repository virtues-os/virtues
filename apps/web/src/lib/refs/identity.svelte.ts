/**
 * A thing's name, icon and color have one owner: the thing. Anything that
 * points at it — a pin today; tabs, ref pills and ⌘K next
 * (agents/plan/names-plan.md) — holds its URL and reads the rest here.
 *
 * Reads overlay the stores that already hold these things (chats, pages,
 * projects) on the server's resolution, so a rename made in this session
 * shows everywhere at once, and a thing the stores do not hold (an old chat,
 * a person) still has a name.
 *
 * Writes go to the thing. Renaming or re-iconing from a pin renames or
 * re-icons the chat, page or project itself — a pin has no name of its own.
 * Only what has no record behind it (an external URL, an app screen) keeps
 * its label and icon on the pin, because there the pin is the thing.
 */

import { updateChat, updatePage, type Pin } from '$lib/api/client';
import { chatSessions } from '$lib/stores/chatSessions.svelte';
import { pagesStore } from '$lib/stores/pages.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { pinsStore } from '$lib/stores/pins.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { parseRef } from '$lib/utils/refRoutes';

export interface Identity {
	kind: string;
	title: string;
	icon: string | null;
	color: string | null;
}

/** The kinds whose name, icon and color live on their own record. */
export type OwnedKind = 'chat' | 'page' | 'project';

export function ownedRef(url: string): { kind: OwnedKind; id: string } | null {
	const ref = parseRef(url);
	if (!ref) return null;
	if (ref.kind === 'chat' || ref.kind === 'page' || ref.kind === 'project') {
		return { kind: ref.kind, id: ref.id };
	}
	return null;
}

/** The fallback name per kind, defined once. */
export function untitled(kind: string): string {
	if (kind === 'chat') return 'New chat';
	if (kind === 'page') return 'Untitled page';
	if (kind === 'project') return 'Untitled project';
	return 'Untitled';
}

/** A chat, page or project as this client's stores hold it right now. */
function storeIdentity(url: string): Identity | null {
	const owned = ownedRef(url);
	if (owned?.kind === 'chat') {
		const s = chatSessions.sessions.find((c) => c.conversation_id === owned.id);
		if (s) return { kind: 'chat', title: s.title?.trim() || untitled('chat'), icon: s.icon, color: s.icon_color ?? null };
	} else if (owned?.kind === 'page') {
		const p = pagesStore.pages.find((x) => x.id === owned.id);
		if (p) return { kind: 'page', title: p.title?.trim() || untitled('page'), icon: p.icon, color: p.icon_color };
	} else if (owned?.kind === 'project') {
		const p = projectStore.projects.find((x) => x.id === owned.id);
		if (p) return { kind: 'project', title: p.name?.trim() || untitled('project'), icon: p.icon, color: p.accent_color ?? null };
	}
	return null;
}

/**
 * What `url` is called and wears now. The live store row when this client
 * holds one; otherwise `known`, the best the caller has — a pin's server
 * resolution, a tab's last-known label. Icon and color fall back to `known`
 * when the thing has none of its own.
 */
export function identityOf(
	url: string,
	known: { kind?: string | null; title?: string | null; icon?: string | null; color?: string | null } = {},
): Identity {
	const live = storeIdentity(url);
	if (live) {
		return { ...live, icon: live.icon ?? known.icon ?? null, color: live.color ?? known.color ?? null };
	}
	const kind = known.kind ?? parseRef(url)?.kind ?? 'route';
	return {
		kind,
		title: known.title?.trim() || (ownedRef(url) ? untitled(kind) : url),
		icon: known.icon ?? null,
		color: known.color ?? null,
	};
}

/** What a pin shows: the thing, else the box's resolution of it. */
export function pinIdentity(pin: Pin): Identity {
	return identityOf(pin.url, {
		kind: pin.kind,
		title: pin.title?.trim() || pin.label?.trim() || pin.url,
		icon: pin.icon,
		color: pin.color,
	});
}

/** Every open tab on the route takes the new name, in every pane. */
export function relabelTabs(route: string, label: string) {
	for (const pane of windowShellStore.panes) {
		for (const tab of pane.tabs) {
			if (tab.route === route) windowShellStore.updateTab(tab.id, { label });
		}
	}
}

/**
 * Rename what `url` points at. Stores and tabs take the name first, so every
 * surface moves together; a failed write puts the server's name back.
 */
export async function renameRef(url: string, title: string): Promise<void> {
	const owned = ownedRef(url);
	relabelTabs(url, title);
	try {
		if (owned?.kind === 'chat') {
			chatSessions.applyTitle(owned.id, title);
			await updateChat(owned.id, { title });
		} else if (owned?.kind === 'page') {
			pagesStore.updatePageLocally(owned.id, { title });
			await updatePage(owned.id, { title });
		} else if (owned?.kind === 'project') {
			await projectStore.update(owned.id, { name: title });
		} else {
			const pin = pinsStore.getByUrl(url);
			if (pin) await pinsStore.setLabel(pin.id, title);
		}
	} finally {
		if (owned?.kind === 'chat') await chatSessions.refresh();
		if (owned?.kind === 'page') await pagesStore.loadPages();
		await pinsStore.load();
	}
}

/** Change the icon, or the icon's color, of what `url` points at. */
export async function setRefLook(
	url: string,
	look: { icon?: string | null; color?: string | null },
): Promise<void> {
	const owned = ownedRef(url);
	if (owned?.kind === 'chat') {
		if ('icon' in look) chatSessions.updateSessionIcon(owned.id, look.icon ?? null);
		if ('color' in look) chatSessions.updateSessionIconColor(owned.id, look.color ?? null);
		await updateChat(owned.id, {
			...('icon' in look ? { icon: look.icon ?? null } : {}),
			...('color' in look ? { icon_color: look.color ?? null } : {}),
		});
	} else if (owned?.kind === 'page') {
		pagesStore.updatePageLocally(owned.id, {
			...('icon' in look ? { icon: look.icon ?? null } : {}),
			...('color' in look ? { icon_color: look.color ?? null } : {}),
		});
		await pagesStore.savePage(owned.id, {
			...('icon' in look ? { icon: look.icon ?? null } : {}),
			...('color' in look ? { icon_color: look.color ?? null } : {}),
		});
	} else if (owned?.kind === 'project') {
		await projectStore.update(owned.id, {
			...('icon' in look ? { icon: look.icon ?? null } : {}),
			...('color' in look ? { accent_color: look.color ?? null } : {}),
		});
	} else {
		const pin = pinsStore.getByUrl(url);
		if (!pin) return;
		if ('icon' in look) await pinsStore.setIcon(pin.id, look.icon ?? null);
		if ('color' in look) await pinsStore.setColor(pin.id, look.color ?? null);
		return;
	}
	await pinsStore.load();
}
