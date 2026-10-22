/**
 * The sidebar's doors, as the owner arranged them.
 *
 * The doors are the places every box has (New chat, Pages, Tasks, Projects,
 * Applets, Wiki). Their arrangement is the owner's: an order and a set of
 * hidden doors, saved on the profile (`sidebar_doors`, migration 0058) so
 * every device shows the same sidebar. Setup is not a door here: it shows
 * while setup is unfinished and leaves when it is done.
 *
 * The catalog below is the shipped arrangement. A saved id this build does
 * not know is skipped, and a door missing from the saved order takes its
 * shipped place, so adding or removing a door never needs a migration.
 *
 * Moves and hides are optimistic and put back if the box refuses them,
 * because the sidebar is the feedback.
 */
import { getProfile, updateProfile } from '$lib/api/client';

export type DoorId = 'new-chat' | 'pages' | 'tasks' | 'projects' | 'applets' | 'wiki';

export interface Door {
	id: DoorId;
	label: string;
	/** Atlas glyph name. */
	glyph: string;
}

export const DOORS: readonly Door[] = [
	{ id: 'new-chat', label: 'New chat', glyph: 'new-chat' },
	{ id: 'pages', label: 'Pages', glyph: 'pages' },
	{ id: 'tasks', label: 'Tasks', glyph: 'tasks' },
	{ id: 'projects', label: 'Projects', glyph: 'projects' },
	{ id: 'applets', label: 'Applets', glyph: 'applets' },
	{ id: 'wiki', label: 'Wiki', glyph: 'wiki' },
];

const byId = new Map(DOORS.map((d) => [d.id, d]));

/**
 * The saved order made whole: known ids as saved, and each door missing from
 * it placed after the door that comes before it in the shipped order (or
 * first, when none does), so a new door appears beside its neighbor.
 */
export function arrange(saved: readonly string[]): Door[] {
	const seen = new Set<string>();
	const out: Door[] = [];
	for (const id of saved) {
		const door = byId.get(id as DoorId);
		if (door && !seen.has(id)) {
			out.push(door);
			seen.add(id);
		}
	}
	DOORS.forEach((door, shipped) => {
		if (seen.has(door.id)) return;
		let at = 0;
		for (let i = shipped - 1; i >= 0; i--) {
			const before = out.findIndex((d) => d.id === DOORS[i].id);
			if (before >= 0) {
				at = before + 1;
				break;
			}
		}
		out.splice(at, 0, door);
		seen.add(door.id);
	});
	return out;
}

class SidebarDoorsStore {
	/** Every door, in the owner's order, hidden ones included. */
	all = $state<Door[]>([...DOORS]);
	hidden = $state<ReadonlySet<DoorId>>(new Set());
	/** Saves in flight. A reload while one is out would put back the old arrangement. */
	private saving = 0;

	/** The doors the sidebar shows. */
	get visible(): Door[] {
		return this.all.filter((d) => !this.hidden.has(d.id));
	}

	/** Whether the owner has changed anything from the shipped arrangement. */
	get customized(): boolean {
		return this.hidden.size > 0 || this.all.some((d, i) => d.id !== DOORS[i].id);
	}

	async load() {
		try {
			const doors = (await getProfile()).sidebar_doors;
			if (this.saving > 0) return;
			if (!doors) {
				this.all = [...DOORS];
				this.hidden = new Set();
				return;
			}
			this.all = arrange(doors.order ?? []);
			this.hidden = new Set((doors.hidden ?? []).filter((id): id is DoorId => byId.has(id as DoorId)));
		} catch (e) {
			console.error('[sidebarDoors] load failed', e);
		}
	}

	/**
	 * Move the shown door at `from` to `to`, both counted among the shown
	 * doors. Hidden doors keep their places, so one shown again comes back
	 * where it was.
	 */
	move(from: number, to: number): Promise<void> {
		const shown = this.visible;
		if (from === to || from < 0 || to < 0 || from >= shown.length || to >= shown.length) return Promise.resolve();
		const next = [...shown];
		const [moved] = next.splice(from, 1);
		next.splice(to, 0, moved);
		let k = 0;
		const all = this.all.map((d) => (this.hidden.has(d.id) ? d : next[k++]));
		return this.save(all, this.hidden);
	}

	setHidden(id: DoorId, hide: boolean): Promise<void> {
		const hidden = new Set(this.hidden);
		if (hide) hidden.add(id);
		else hidden.delete(id);
		return this.save(this.all, hidden);
	}

	reset(): Promise<void> {
		return this.save([...DOORS], new Set());
	}

	private async save(all: Door[], hidden: ReadonlySet<DoorId>) {
		const previous = { all: this.all, hidden: this.hidden };
		this.all = all;
		this.hidden = hidden;
		this.saving++;
		try {
			await updateProfile({ sidebar_doors: { order: all.map((d) => d.id), hidden: [...hidden] } });
		} catch (e) {
			this.all = previous.all;
			this.hidden = previous.hidden;
			throw e;
		} finally {
			this.saving--;
		}
	}
}

export const sidebarDoors = new SidebarDoorsStore();
