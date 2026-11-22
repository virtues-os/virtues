/**
 * Notes from `GET /api/notes`: what your server noticed about your tasks and
 * threads. The Threads door's Insights view lists them, a preview shows the
 * ones about its thread or task, rows carry a mark, and home's notes line
 * shows them in turn. A view that shows notes calls `start` while it is
 * mounted; the list reloads every minute while anything is watching.
 */
import { dismissNote, getNotes, restoreNote, type Note } from '$lib/api/client';
import { toast } from 'svelte-sonner';

const RELOAD_MS = 60_000;

class NotesStore {
	items = $state<Note[]>([]);
	/** The list has arrived at least once, so an empty one is really empty. */
	loaded = $state(false);
	/** The last load failed; the list keeps what it had. */
	failed = $state(false);
	/** Counts loads, so a view showing one note can read it again after Undo. */
	revision = $state(0);
	private watchers = 0;
	private timer: ReturnType<typeof setInterval> | null = null;

	async load(): Promise<void> {
		try {
			this.items = await getNotes();
			this.loaded = true;
			this.failed = false;
			this.revision += 1;
		} catch (e) {
			console.error('[notes] load failed:', e);
			this.failed = true;
		}
	}

	start(): void {
		this.watchers += 1;
		if (this.watchers > 1) return;
		void this.load();
		this.timer = setInterval(() => {
			if (document.visibilityState === 'visible') void this.load();
		}, RELOAD_MS);
	}

	stop(): void {
		this.watchers = Math.max(0, this.watchers - 1);
		if (this.watchers === 0 && this.timer) {
			clearInterval(this.timer);
			this.timer = null;
		}
	}

	/** The notes about a route (`/thread/…`, `/task/…`). */
	about(url: string): Note[] {
		return this.items.filter((n) => n.subjects.includes(url));
	}

	/** Not now: hide it here at once, set it aside on the server, offer it back. */
	async dismiss(note: Note): Promise<void> {
		this.items = this.items.filter((n) => n.key !== note.key);
		try {
			await dismissNote(note.key);
			toast('Set aside', {
				action: {
					label: 'Undo',
					onClick: () =>
						void restoreNote(note.key)
							.then(() => this.load())
							.catch(() => toast.error("Your server couldn't bring that back", { description: 'Try again' })),
				},
			});
		} catch (e) {
			toast.error("Your server couldn't set that aside", { description: 'Try again' });
			await this.load();
		}
	}
}

export const notesStore = new NotesStore();
