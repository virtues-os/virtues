/**
 * The New task dialog: one for the whole app, opened from the Threads door,
 * a thread's page, a thing's page, the sidebar, the New menu and ⌘⇧E. A
 * place that opens it can give the new task a home.
 */
import type { TaskLink } from '$lib/api/client';

class NewTaskStore {
	isOpen = $state(false);
	/** The home a new task starts with, from the page that opened the dialog. */
	home = $state<TaskLink | null>(null);

	open(opts: { home?: TaskLink | null } = {}) {
		this.home = opts.home ?? null;
		this.isOpen = true;
	}

	close() {
		this.isOpen = false;
	}
}

export const newTaskDialog = new NewTaskStore();
