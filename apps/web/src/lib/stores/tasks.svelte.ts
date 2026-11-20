/**
 * The task list, shared by every view that shows it. Open tasks load with
 * the view; closed ones load only when Done is unfolded, because the list
 * only ever grows there.
 *
 * Checking and dismissing are optimistic: the row moves at once and moves
 * back, with a toast, if the server refuses.
 */
import {
	addTaskNote,
	createTask,
	listTasks,
	updateTask,
	type NewTask,
	type Task,
	type TaskPatch,
} from '$lib/api/client';

class TasksStore {
	open = $state<Task[]>([]);
	closed = $state<Task[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);
	/** The open tasks have arrived at least once, so an empty list is really empty. */
	loaded = $state(false);
	closedLoaded = $state(false);
	async load() {
		this.loading = true;
		this.error = null;
		try {
			this.open = await listTasks('open');
			this.loaded = true;
			if (this.closedLoaded) this.closed = await listTasks('closed');
		} catch (e) {
			this.error = e instanceof Error ? e.message : 'Could not load tasks';
		} finally {
			this.loading = false;
		}
	}

	/**
	 * Reload without the loading state, for the page's live refresh. The
	 * server works requests in the background, so the list changes without
	 * anyone here touching it.
	 */
	async refresh() {
		try {
			this.open = await listTasks('open');
			this.loaded = true;
			if (this.closedLoaded) this.closed = await listTasks('closed');
			this.error = null;
		} catch {
			// The next refresh tries again; the list keeps what it showed.
		}
	}

	/** Whether the server has a request queued or running. */
	get working(): boolean {
		return this.open.some(
			(t) => t.assignee === 'assistant' && (t.status === 'active' || (t.status === 'open' && !!t.next_run_at)),
		);
	}

	/**
	 * Answer a ready request: the reply goes into its history and the
	 * request goes back in the queue, to run again in the same chat.
	 */
	async reply(id: string, text: string): Promise<Task> {
		await addTaskNote(id, text);
		return this.update(id, { status: 'open' });
	}

	async loadClosed() {
		this.closed = await listTasks('closed');
		this.closedLoaded = true;
	}

	async create(task: NewTask): Promise<Task> {
		const created = await createTask(task);
		this.open = [created, ...this.open];
		return created;
	}

	/** Apply `patch`, showing it before the server answers. */
	async update(id: string, patch: TaskPatch): Promise<Task> {
		const before = this.find(id);
		if (before) this.place({ ...before, ...patch } as Task);
		try {
			const saved = await updateTask(id, patch);
			this.place(saved);
			// Finishing a recurring task opens the next one on the server, and
			// reopening it takes that one back, so the list is read again.
			if (patch.status && saved.recur_every) await this.refresh();
			return saved;
		} catch (e) {
			if (before) this.place(before);
			throw e;
		}
	}

	find(id: string): Task | undefined {
		return this.open.find((t) => t.id === id) ?? this.closed.find((t) => t.id === id);
	}

	/** Put `task` in the list its status belongs to, replacing any copy. */
	private place(task: Task) {
		const isClosed = task.status === 'done' || task.status === 'dismissed';
		this.open = this.open.filter((t) => t.id !== task.id);
		this.closed = this.closed.filter((t) => t.id !== task.id);
		if (isClosed) {
			this.closed = [task, ...this.closed];
		} else {
			this.open = [...this.open, task];
		}
	}
}

export const tasksStore = new TasksStore();
