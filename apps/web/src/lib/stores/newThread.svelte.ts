/**
 * The New thread dialog: one for the whole app, opened from the Threads
 * door, the sidebar, the New menu and a thing's page. A thing's page opens
 * it with the thing the thread will sit under; a caller can also say where
 * to open the thread once it is made.
 */
type Open = (route: string, label: string) => void;

class NewThreadStore {
	isOpen = $state(false);
	/** The thing the new thread starts under, from the page that opened the dialog. */
	under = $state<string | null>(null);
	/** How to show the new thread; a new tab when unset. */
	onOpen: Open | null = null;

	open(opts: { under?: string | null; onOpen?: Open } = {}) {
		this.under = opts.under ?? null;
		this.onOpen = opts.onOpen ?? null;
		this.isOpen = true;
	}

	close() {
		this.isOpen = false;
	}
}

export const newThreadDialog = new NewThreadStore();
