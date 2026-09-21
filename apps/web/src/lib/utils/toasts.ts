/**
 * Result toasts for archive and delete — the one place the app says what just
 * happened to something the person put away.
 *
 * Both actions used to be silent on success: a confirm dialog, then the row
 * vanished and (for a project or chat) the tab closed under you. The only
 * evidence the box had done anything was an absence.
 *
 * Every delete here is soft — Recently deleted holds it 30 days — so the toast
 * carries the way back. `Undo` restores it now; `View` opens the room. The
 * toast expiring costs nothing: both doors stay open for the full 30 days.
 *
 * On svelte-sonner, which the (app) layout already mounts. `cancel` renders
 * before `action`, so the buttons read "View  Undo" with Undo emphasized.
 * Deliberately NOT `toast.success` — the `data-type="success"` rule in app.css
 * paints the whole toast green, and putting a project away is not a victory.
 */
import { toast } from 'svelte-sonner';
import {
	restoreTrashed,
	restoreDriveFile,
	type TrashKind,
} from '$lib/api/client';
import { chatSessions } from '$lib/stores/chatSessions.svelte';
import { pagesStore } from '$lib/stores/pages.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';

/** Everything Recently deleted holds: the three record kinds, plus Drive files. */
export type TrashedKind = TrashKind | 'file';

/** Where Recently deleted lives. `/trash` still resolves, but this is the name. */
const TRASH_ROUTE = '/storage/trash';

/**
 * Long enough to read the line and reach for Undo. Matches the update toast in
 * the (app) layout, the other one with a button on it.
 */
const UNDO_DURATION = 8000;

/** A title in a toast, quoted and kept short enough not to wrap the toast. */
function quoted(name: string | null | undefined): string {
	const trimmed = (name ?? '').trim();
	if (!trimmed) return 'it';
	return `"${trimmed.length > 48 ? `${trimmed.slice(0, 47)}…` : trimmed}"`;
}

/**
 * The route, but only if a tab is showing it right now.
 *
 * Call this BEFORE the delete. Deleting closes the tabs for the thing, so an
 * Undo that restores the record leaves it invisible unless the tab comes back
 * — and reopening unconditionally is just as wrong, because it would conjure a
 * tab the person never had open.
 */
export function routeIfOpen(route: string): string | undefined {
	return windowShellStore.findTab((t) => t.route === route) ? route : undefined;
}

/**
 * Reload whatever store lists this kind, so a restored thing reappears at once.
 * Shared with Recently deleted's own restore button — one list of stores to
 * poke, not two that have to agree.
 */
export async function refreshAfterRestore(kind: TrashedKind): Promise<void> {
	if (kind === 'chat') await chatSessions.refresh();
	else if (kind === 'page') await pagesStore.loadPages();
	else if (kind === 'project') await projectStore.load();
	windowShellStore.invalidateViewCache(kind === 'file' ? 'storage' : kind);
}

function openTrash(): void {
	windowShellStore.openTabFromRoute(TRASH_ROUTE, { focusExisting: true });
}

export interface TrashedOptions {
	/** What was deleted, for the restore call and the store refresh. */
	kind: TrashedKind;
	id: string;
	name: string | null | undefined;
	/**
	 * A route to reopen on Undo — pass `routeIfOpen(route)` from before the
	 * delete, so a tab comes back only if there was one.
	 */
	reopen?: string;
	/** Extra work after a successful Undo, for a view holding its own list. */
	onRestored?: () => void | Promise<void>;
	/**
	 * Set false where one Undo cannot put back everything the delete took, and
	 * the toast offers only the room. A Drive folder is the case: deleting one
	 * trashes everything inside it, but restoring it brings back the folder
	 * alone, so an Undo here would hand back an empty shell.
	 */
	undoable?: boolean;
}

async function undoTrash(opts: TrashedOptions): Promise<void> {
	try {
		if (opts.kind === 'file') await restoreDriveFile(opts.id);
		else await restoreTrashed(opts.kind, opts.id);
		await refreshAfterRestore(opts.kind);
		await opts.onRestored?.();
		if (opts.reopen) {
			windowShellStore.openTabFromRoute(opts.reopen, { focusExisting: true });
		}
	} catch (e) {
		console.error('[toasts] restore failed:', e);
		// Name the actor, and say the thing is safe: the restore call failed,
		// the record did not go anywhere.
		toast.error(`Your server couldn't restore ${quoted(opts.name)}`, {
			description: "It's still in Recently deleted. Try again from there",
			action: { label: 'View', onClick: openTrash },
		});
	}
}

/**
 * Something went to Recently deleted. Offers Undo, and View for the room.
 *
 * Call it after the delete has succeeded — a toast that says "Deleted" before
 * the box agrees is a lie you have to take back.
 */
export function notifyTrashed(opts: TrashedOptions): void {
	const undoable = opts.undoable !== false;
	toast(`Deleted ${quoted(opts.name)}`, {
		description: 'In Recently deleted for 30 days',
		duration: UNDO_DURATION,
		// With no Undo, the room is the only way back, so it stops being the
		// quiet second button and becomes the one the toast is offering.
		cancel: undoable ? { label: 'View', onClick: openTrash } : undefined,
		action: undoable
			? { label: 'Undo', onClick: () => void undoTrash(opts) }
			: { label: 'View', onClick: openTrash },
	});
}

/**
 * A project was archived. No View: archiving leaves it open in front of you,
 * and the fold it went to is one click down the projects list.
 */
export function notifyArchived(id: string, name: string | null | undefined): void {
	toast(`Archived ${quoted(name)}`, {
		description: 'Find it under Archived on your projects list',
		duration: UNDO_DURATION,
		action: {
			label: 'Undo',
			onClick: () => {
				void projectStore.unarchive(id).catch((e) => {
					console.error('[toasts] unarchive failed:', e);
					toast.error(`Your server couldn't reopen ${quoted(name)}`, {
						description: 'Try Unarchive on your projects list',
					});
				});
			},
		},
	});
}
