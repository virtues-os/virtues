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
	private watchers = 0;
	private timer: ReturnType<typeof setInterval> | null = null;

	async load(): Promise<void> {
		try {
			this.items = await getNotes();
		} catch (e) {
			// A server older than notes has no list; nothing is shown.
			console.error('[notes] load failed:', e);
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
					onClick: () => void restoreNote(note.key).then(() => this.load()),
				},
			});
		} catch (e) {
			toast.error("Your server couldn't set that aside", { description: 'Try again' });
			await this.load();
		}
	}
}

export const notesStore = new NotesStore();
