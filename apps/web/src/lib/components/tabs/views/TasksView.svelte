<script lang="ts">
	/**
	 * Tasks (agents/plan/tasks-plan.md): the owner's to-dos and their
	 * requests to the assistant on one page, top to bottom:
	 *
	 *   Ready     requests whose result waits for the owner, with the result
	 *             on the page. Accept closes one; Reply runs it again.
	 *   Working   requests queued or running on the server, with the model
	 *             doing the work and who pays for it.
	 *   To-dos    the owner's own, dated ones first. Group headers appear only
	 *             once the list is long enough to need them.
	 *   Done      folded: accepted requests, finished and dismissed tasks.
	 *
	 * Return in the add row makes a to-do; Mod+Return makes a request. A row
	 * opens in place, with its title editable in the row. No counts, no
	 * pills and no red anywhere: the list must never nag.
	 *
	 * The server works requests in the background, so while one is queued or
	 * running the page reloads the list every few seconds.
	 */
	import type { Tab } from "$lib/tabs/types";
	import { addTaskNote, getByoKey, listTaskEvents, type Task, type TaskEvent, type TaskPatch } from "$lib/api/client";
	import { tasksStore } from "$lib/stores/tasks.svelte";
	import { projectStore } from "$lib/stores/project.svelte";
	import { pagesStore } from "$lib/stores/pages.svelte";
	import { modelSlots } from "$lib/stores/modelSlots.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { isAppleKeyboard } from "$lib/utils/platform";
	import { formatMicrosUSD } from "$lib/utils/currency";
	import { Page } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import Markdown from "$lib/components/Markdown.svelte";
	import RefCard from "$lib/components/RefCard.svelte";
	import Popover from "$lib/floating/primitives/Popover.svelte";
	import TaskComposer from "$lib/components/tasks/TaskComposer.svelte";
	import DuePicker from "$lib/components/tasks/DuePicker.svelte";
	import { dueGroup, dueLabel, isoDay, parseDue, type DueGroup } from "$lib/tasks/due";
	import { toast } from "svelte-sonner";
	import { onMount, tick } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	/** Past this many to-dos, they are grouped under date headers. */
	const GROUP_FROM = 10;

	/** How often the page reloads while the server has a request queued or running, and otherwise. */
	const POLL_WORKING_MS = 4_000;
	const POLL_IDLE_MS = 30_000;

	const GROUPS: { id: DueGroup; label: string }[] = [
		{ id: "overdue", label: "Earlier" },
		{ id: "today", label: "Today" },
		{ id: "upcoming", label: "Coming up" },
		{ id: "none", label: "No date" },
	];

	/** Plain string order. `localeCompare` sorts "~" before digits, and "~" stands for "no date". */
	function cmp(a: string, b: string): number {
		return a < b ? -1 : a > b ? 1 : 0;
	}

	/** The day rolls over while the view is open; dates relabel when it does. */
	let today = $state(isoDay(new Date()));

	// ── what the page shows ─────────────────────────────────────────────────

	const requests = $derived(tasksStore.open.filter((t) => t.assignee === "assistant"));
	const ready = $derived(
		requests.filter((t) => t.status === "ready").sort((a, b) => b.updated_at.localeCompare(a.updated_at)),
	);
	const working = $derived(
		requests
			.filter((t) => t.status !== "ready")
			.sort(
				(a, b) =>
					Number(b.status === "active") - Number(a.status === "active") ||
					cmp(a.next_run_at ?? "~", b.next_run_at ?? "~") ||
					b.created_at.localeCompare(a.created_at),
			),
	);
	const todos = $derived.by(() => {
		void today;
		return tasksStore.open
			.filter((t) => t.assignee === "owner")
			.sort(
				(a, b) =>
					cmp(a.due_on ?? "~", b.due_on ?? "~") ||
					cmp(a.due_at ?? "", b.due_at ?? "") ||
					a.sort_order - b.sort_order ||
					b.created_at.localeCompare(a.created_at),
			);
	});
	const todoGroups = $derived.by(() => {
		if (todos.length <= GROUP_FROM) return [{ id: "all", label: null as string | null, tasks: todos }];
		const now = new Date();
		return GROUPS.map((g) => ({
			id: g.id as string,
			label: g.label as string | null,
			tasks: todos.filter((t) => dueGroup(t.due_on, now) === g.id),
		})).filter((g) => g.tasks.length > 0);
	});
	const closed = $derived(tasksStore.closed);
	const isEmpty = $derived(tasksStore.open.length === 0);

	let doneOpen = $state(false);

	/** Every row on the page in reading order, for j and k. */
	const order = $derived([
		...ready.map((t) => t.id),
		...working.map((t) => t.id),
		...todos.map((t) => t.id),
		...(doneOpen ? closed.map((t) => t.id) : []),
	]);

	const projectName = $derived.by(() => {
		const names = new Map<string, string>();
		for (const p of [...projectStore.projects, ...projectStore.archived]) names.set(p.id, p.name);
		return names;
	});

	// ── loading and live refresh ────────────────────────────────────────────

	$effect(() => {
		if (active) void tasksStore.load();
	});

	$effect(() => {
		if (!active) return;
		const every = tasksStore.working ? POLL_WORKING_MS : POLL_IDLE_MS;
		const timer = setInterval(() => {
			if (document.visibilityState === "visible") void refresh();
		}, every);
		const onVisible = () => {
			if (document.visibilityState === "visible") void refresh();
		};
		document.addEventListener("visibilitychange", onVisible);
		return () => {
			clearInterval(timer);
			document.removeEventListener("visibilitychange", onVisible);
		};
	});

	async function refresh() {
		await tasksStore.refresh();
		if (expanded) void loadHistory(expanded);
	}

	/** Where BYO calls go: this server, a machine on its network, or a provider. */
	let byoPlace = $state("on your own account");

	onMount(() => {
		if (!projectStore.loaded) void projectStore.load();
		if (!modelSlots.loaded) void modelSlots.load();
		void getByoKey<{ endpoint_url?: string | null }>()
			.then((s) => (byoPlace = placeOf(s.endpoint_url)))
			.catch(() => {
				// The line keeps its general wording.
			});
		const timer = setInterval(() => (today = isoDay(new Date())), 60_000);
		return () => clearInterval(timer);
	});

	function placeOf(endpoint: string | null | undefined): string {
		let host = "";
		try {
			host = endpoint ? new URL(endpoint).hostname : "";
		} catch {
			return "on your own account";
		}
		if (host === "localhost" || host === "::1" || host === "[::1]" || host.startsWith("127.")) return "on your server";
		if (/^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)/.test(host) || host.endsWith(".local")) return "on your network";
		return "on your own account";
	}

	/** The model a request's latest run used, and who paid for it. */
	function whereItRan(t: Task): string | null {
		if (!t.run_model) return null;
		const name = modelSlots.nameOf(t.run_model);
		return t.run_route === "byo" ? `${name} ${byoPlace}` : `${name} through Virtues`;
	}

	/** A request's state in a word or two. */
	function requestState(t: Task): string {
		if (t.status === "ready") return "Ready";
		if (t.status === "active") return "Working";
		if (t.status === "open" && t.next_run_at) {
			const at = new Date(t.next_run_at);
			if (at.getTime() - Date.now() > 60_000) {
				return `Starts ${dueLabel(isoDay(at), at.toISOString())}`;
			}
			return "Queued";
		}
		return "Not started";
	}

	// ── adding ──────────────────────────────────────────────────────────────

	let draft = $state("");
	let addInput: HTMLInputElement | undefined = $state();
	const preview = $derived(parseDue(draft));
	const askKeys = isAppleKeyboard ? "⌘Return" : "Ctrl+Enter";

	// A "New task" door asks for the add row: the sidebar +, ⌘⇧E.
	let seenAddRequest = tasksStore.addRequest;
	$effect(() => {
		if (tasksStore.addRequest !== seenAddRequest && active) {
			seenAddRequest = tasksStore.addRequest;
			void tick().then(() => addInput?.focus());
		}
	});

	/**
	 * The row clears the moment Return is pressed and stays live, so the
	 * next one can be typed straight away; a refused task comes back into
	 * the row if nothing new has been typed there.
	 */
	async function add(asRequest: boolean) {
		const { title, due_on, due_at } = preview;
		if (!title) return;
		const typed = draft;
		draft = "";
		try {
			await tasksStore.create({ title, due_on, due_at, assignee: asRequest ? "assistant" : "owner" });
		} catch (e) {
			if (!draft) draft = typed;
			toast.error("Your server couldn't add that", { description: errorText(e) });
		}
	}

	function addKeydown(e: KeyboardEvent) {
		if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
			e.preventDefault();
			void add(true);
		}
	}

	// ── changing ────────────────────────────────────────────────────────────

	async function save(id: string, patch: TaskPatch, failure: string) {
		try {
			await tasksStore.update(id, patch);
		} catch (e) {
			toast.error(failure, { description: errorText(e) });
		}
	}

	function isClosed(t: Task): boolean {
		return t.status === "done" || t.status === "dismissed";
	}

	function toggleDone(t: Task) {
		void save(t.id, { status: isClosed(t) ? "open" : "done" }, "Your server couldn't update that task");
	}

	function accept(t: Task) {
		void save(t.id, { status: "done" }, "Your server couldn't close that request");
	}

	function dismiss(t: Task) {
		if (expanded === t.id) expanded = null;
		void save(t.id, { status: "dismissed" }, "Your server couldn't dismiss that");
		toast("Dismissed", {
			action: { label: "Undo", onClick: () => void save(t.id, { status: "open" }, "Your server couldn't restore that") },
		});
	}

	/** Give a to-do to the assistant: the server queues it and works it in the background. */
	async function handOver(t: Task) {
		await save(t.id, { assignee: "assistant" }, "Your server couldn't hand that over");
		if (expanded === t.id) void loadHistory(t.id);
	}

	async function takeBack(t: Task) {
		await save(t.id, { assignee: "owner" }, "Your server couldn't take that back");
		if (expanded === t.id) void loadHistory(t.id);
	}

	function saveTitle(t: Task, value: string) {
		const title = value.trim();
		if (title && title !== t.title) void save(t.id, { title }, "Your server couldn't rename that");
	}

	async function saveNotes(t: Task, value: string) {
		const notes = value.trim() || null;
		if (notes !== (t.notes ?? null)) await save(t.id, { notes }, "Your server couldn't save those notes");
	}

	function saveDue(t: Task, due_on: string | null, due_at: string | null) {
		if (due_on === t.due_on && due_at === t.due_at) return;
		void save(t.id, { due_on, due_at }, "Your server couldn't change that date");
	}

	function saveProject(t: Task, project_id: string | null) {
		if (project_id !== t.project_id) void save(t.id, { project_id }, "Your server couldn't move that");
	}

	/** Reply to a ready request: it runs again in the same work chat. */
	async function sendReply(t: Task, text: string): Promise<boolean> {
		const said = text.trim();
		if (!said) return false;
		try {
			await tasksStore.reply(t.id, said);
			replying = null;
			return true;
		} catch (e) {
			toast.error("Your server couldn't send that reply", { description: errorText(e) });
			return false;
		}
	}

	/** Add a note to a request that is still with the assistant; its next run reads it. */
	async function addNote(t: Task, text: string): Promise<boolean> {
		const said = text.trim();
		if (!said) return false;
		try {
			await addTaskNote(t.id, said);
			void loadHistory(t.id);
			return true;
		} catch (e) {
			toast.error("Your server couldn't add that note", { description: errorText(e) });
			return false;
		}
	}

	function errorText(e: unknown): string | undefined {
		return e instanceof Error ? e.message : undefined;
	}

	// ── done fold ───────────────────────────────────────────────────────────

	async function toggleDoneFold() {
		doneOpen = !doneOpen;
		if (doneOpen && !tasksStore.closedLoaded) {
			try {
				await tasksStore.loadClosed();
			} catch (e) {
				toast.error("Your server couldn't load finished tasks", { description: errorText(e) });
			}
		}
	}

	// ── opening a row ───────────────────────────────────────────────────────

	let expanded = $state<string | null>(null);
	let replying = $state<string | null>(null);
	let focused = $state<string | null>(null);
	let history = $state<TaskEvent[]>([]);
	let duePickerOpen = $state(false);
	let projectPickerOpen = $state(false);

	function open(id: string | null, focusTitle = false) {
		expanded = id;
		history = [];
		duePickerOpen = false;
		projectPickerOpen = false;
		if (!id) return;
		focused = id;
		void loadHistory(id);
		if (focusTitle) void tick().then(() => titleInput(id)?.focus());
	}

	function toggle(t: Task) {
		open(expanded === t.id ? null : t.id);
	}

	function rowEl(id: string): HTMLElement | null {
		return document.querySelector<HTMLElement>(`[data-task-id="${CSS.escape(id)}"]`);
	}

	function titleInput(id: string): HTMLInputElement | null {
		return rowEl(id)?.querySelector<HTMLInputElement>("input.title-input") ?? null;
	}

	// `?task=<id>` opens that task. Read once per route, so closing the task
	// afterwards keeps it closed.
	const linkedTask = $derived(new URL(tab.route, "http://localhost").searchParams.get("task"));
	let openedLink: string | null = null;
	$effect(() => {
		const id = linkedTask;
		if (!id || id === openedLink || !tasksStore.find(id)) return;
		openedLink = id;
		open(id);
		void tick().then(() => rowEl(id)?.scrollIntoView({ block: "nearest" }));
	});

	async function loadHistory(id: string) {
		try {
			const events = await listTaskEvents(id);
			if (expanded === id) history = events;
		} catch (e) {
			toast.error("Your server couldn't load this task's history", { description: errorText(e) });
		}
	}

	// ── keyboard ────────────────────────────────────────────────────────────

	function typing(target: EventTarget | null): boolean {
		const el = target as HTMLElement | null;
		return !!el && (el.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName));
	}

	function move(by: number) {
		if (order.length === 0) return;
		const at = focused ? order.indexOf(focused) : -1;
		const next = order[Math.min(order.length - 1, Math.max(0, at === -1 ? 0 : at + by))];
		focused = next;
		rowEl(next)?.focus();
	}

	function onKeydown(e: KeyboardEvent) {
		if (!active || e.defaultPrevented) return;
		// A picker that is open takes its own Escape.
		if (duePickerOpen || projectPickerOpen) return;
		if (e.key === "Escape" && expanded) {
			const id = expanded;
			open(null);
			(document.activeElement as HTMLElement | null)?.blur();
			rowEl(id)?.focus();
			return;
		}
		if (typing(e.target) || e.altKey) return;
		const t = focused ? tasksStore.find(focused) : undefined;
		const mod = e.metaKey || e.ctrlKey;
		if (!mod && (e.key === "j" || e.key === "ArrowDown")) {
			e.preventDefault();
			move(1);
		} else if (!mod && (e.key === "k" || e.key === "ArrowUp")) {
			e.preventDefault();
			move(-1);
		} else if (!t) {
			return;
		} else if (e.key === "Enter" && mod) {
			e.preventDefault();
			if (t.assignee === "owner" && !isClosed(t)) void handOver(t);
		} else if (e.key === "Enter") {
			e.preventDefault();
			toggle(t);
		} else if (e.key === "x" && !mod) {
			e.preventDefault();
			if (t.status === "ready") accept(t);
			else if (t.assignee === "owner" || isClosed(t)) toggleDone(t);
		} else if (e.key === "e" && !mod) {
			e.preventDefault();
			open(t.id, true);
		}
	}

	// ── history ─────────────────────────────────────────────────────────────

	/** One line of history, as the owner reads it. */
	function eventLine(e: TaskEvent): string {
		const who = e.author === "assistant" ? "The assistant" : "You";
		const body = e.body as Record<string, unknown>;
		switch (e.kind) {
			case "created":
				return `${who} added this`;
			case "status": {
				const to = String(body.to ?? "");
				const said: Record<string, string> = {
					ready: "finished a run and left a result",
					done: e.author === "owner" ? "closed it" : "finished it",
					dismissed: "dismissed it",
					active: "started working on it",
					open: e.author === "owner" ? "sent it back" : "put it back in the queue",
				};
				return `${who} ${said[to] ?? "changed its status"}`;
			}
			case "edited": {
				const fields = Object.keys(body);
				if (fields.length === 1 && fields[0] === "assignee") {
					return body.assignee === "assistant" ? `${who} handed it to the assistant` : `${who} took it back`;
				}
				const names: Record<string, string> = {
					title: "title",
					notes: "notes",
					assignee: "assignee",
					due_on: "date",
					due_at: "time",
					project_id: "project",
					result_url: "result",
				};
				const changed = [...new Set(fields.map((f) => names[f] ?? f))];
				return `${who} changed the ${changed.join(" and ")}`;
			}
			default:
				return String(body.text ?? "");
		}
	}

	function eventTime(e: TaskEvent): string {
		return new Date(e.created_at).toLocaleString("en-US", {
			month: "short",
			day: "numeric",
			hour: "numeric",
			minute: "2-digit",
		});
	}

	function openRoute(url: string) {
		windowShellStore.openTabFromRoute(url, { focusExisting: true });
	}

	function sourceLabel(url: string): string {
		const kind = url.split("/")[1];
		return { page: "a page", chat: "a chat", project: "a project", day: "a day" }[kind] ?? "a link";
	}

	/** A result URL's label for its card: the page's title when this box has it. */
	function resultLabel(url: string): string {
		const id = url.slice(url.lastIndexOf("/") + 1);
		return pagesStore.pages.find((p) => p.id === id)?.title || "The page it made";
	}
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full w-full flex-col bg-background">
	<div class="min-h-0 flex-1">
		<Page title="Tasks" maxWidth="prose">
			<form
				class="add-row"
				onsubmit={(e) => {
					e.preventDefault();
					void add(false);
				}}
			>
				<span class="add-mark" aria-hidden="true"><Icon icon="ri:add-line" width="16" /></span>
				<input
					bind:this={addInput}
					bind:value={draft}
					onkeydown={addKeydown}
					class="add-input"
					placeholder="Add a to-do"
					aria-label="New to-do"
					aria-describedby="add-hint"
				/>
				{#if preview.due_on && preview.title}
					<span class="add-due" title="Read from what you typed">{dueLabel(preview.due_on, preview.due_at)}</span>
				{/if}
			</form>
			<p class="add-hint" id="add-hint">
				Return adds it to your list. {askKeys} asks the assistant to do it.
			</p>

			{#if tasksStore.loading && tasksStore.open.length === 0}
				<p class="quiet">Loading tasks…</p>
			{:else if tasksStore.error && tasksStore.open.length === 0}
				<p class="quiet">
					Your server couldn't load your tasks.
					<button type="button" class="text-btn" onclick={() => tasksStore.load()}>Try again</button>
				</p>
			{:else if isEmpty}
				<p class="quiet">
					Nothing here yet. A date at the end, like "fri" or "tomorrow 5pm", sets when it's due.
				</p>
			{/if}

			{#if ready.length > 0}
				<section class="section" aria-labelledby="ready-label">
					<h2 class="section-label" id="ready-label">Ready</h2>
					<ul class="rows">
						{#each ready as t (t.id)}
							<li
								class="row ready"
								data-task-id={t.id}
								tabindex="-1"
								class:focused={focused === t.id}
								onfocus={() => (focused = t.id)}
							>
								{@render line(t)}
								<div class="result">
									<Markdown content={t.result ?? ""} />
									{#if t.result_url}
										<RefCard type={t.result_url.split("/")[1] ?? null} label={resultLabel(t.result_url)} url={t.result_url} />
									{/if}
								</div>
								{#if replying === t.id}
									<div class="reply">
										<TaskComposer
											label="Reply"
											placeholder="What should it change or try next?"
											sendLabel="Send"
											autofocus
											onCommit={(text) => sendReply(t, text)}
										/>
									</div>
								{/if}
								<div class="actions">
									<button type="button" class="action strong" onclick={() => accept(t)}>Accept</button>
									<button
										type="button"
										class="action"
										onclick={() => (replying = replying === t.id ? null : t.id)}
									>
										{replying === t.id ? "Cancel reply" : "Reply"}
									</button>
								</div>
								{#if expanded === t.id}
									{@render detail(t)}
								{/if}
							</li>
						{/each}
					</ul>
				</section>
			{/if}

			{#if working.length > 0}
				<section class="section" aria-labelledby="working-label">
					<h2 class="section-label" id="working-label">Working</h2>
					<ul class="rows">
						{#each working as t (t.id)}
							{@render row(t)}
						{/each}
					</ul>
				</section>
			{/if}

			{#if todos.length > 0}
				<section class="section" aria-labelledby="todos-label">
					<h2 class="section-label" id="todos-label">To-dos</h2>
					{#each todoGroups as group (group.id)}
						{#if group.label}
							<h3 class="group-label">{group.label}</h3>
						{/if}
						<ul class="rows">
							{#each group.tasks as t (t.id)}
								{@render row(t)}
							{/each}
						</ul>
					{/each}
				</section>
			{/if}

			<section class="section done-section">
				<button type="button" class="section-label fold" aria-expanded={doneOpen} onclick={toggleDoneFold}>
					<Icon icon={doneOpen ? "ri:arrow-down-s-line" : "ri:arrow-right-s-line"} width="14" />
					Done
				</button>
				{#if doneOpen}
					{#if closed.length === 0 && tasksStore.closedLoaded}
						<p class="quiet">Accepted requests and finished to-dos show up here.</p>
					{/if}
					<ul class="rows">
						{#each closed as t (t.id)}
							{@render row(t)}
						{/each}
					</ul>
				{/if}
			</section>
		</Page>
	</div>
</div>

{#snippet mark(t: Task)}
	<span class="mark" class:working={t.status === "active"} aria-hidden="true">
		<svg viewBox="0 0 16 16" width="14" height="14"
			><circle cx="8" cy="4" r="2" /><circle cx="4" cy="11" r="2" /><circle cx="12" cy="11" r="2" /></svg
		>
	</span>
{/snippet}

{#snippet line(t: Task)}
	{@const request = t.assignee === "assistant" && !isClosed(t)}
	{@const project = t.project_id ? projectName.get(t.project_id) : undefined}
	<div class="line">
		{#if request}
			{@render mark(t)}
		{:else}
			<button
				type="button"
				class="check"
				class:checked={t.status === "done"}
				role="checkbox"
				aria-checked={t.status === "done"}
				aria-label={isClosed(t) ? `Reopen ${t.title}` : `Complete ${t.title}`}
				onclick={() => toggleDone(t)}
			>
				{#if t.status === "done"}
					<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"
						><path d="M4 8.3 6.7 11 12 5.2" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg
					>
				{/if}
			</button>
		{/if}
		{#if expanded === t.id && !isClosed(t)}
			<input
				class="title-input"
				value={t.title}
				aria-label="Title"
				onblur={(e) => saveTitle(t, e.currentTarget.value)}
				onkeydown={(e) => {
					if (e.key === "Enter") e.currentTarget.blur();
				}}
			/>
		{:else}
			<button type="button" class="title" aria-expanded={expanded === t.id} onclick={() => toggle(t)}>
				{t.title}
			</button>
		{/if}
		<button type="button" class="meta" tabindex="-1" aria-hidden="true" onclick={() => toggle(t)}>
			{#if request && t.status !== "ready"}
				<span>{requestState(t)}</span>
			{:else if t.status === "dismissed"}
				<span>Dismissed</span>
			{:else if t.due_on && !isClosed(t)}
				<span>{dueLabel(t.due_on, t.due_at)}</span>
			{/if}
			{#if project}
				<span>{project}</span>
			{/if}
		</button>
		{#if !isClosed(t)}
			<button type="button" class="dismiss" aria-label={`Dismiss ${t.title}`} title="Dismiss" onclick={() => dismiss(t)}>
				<Icon icon="ri:close-line" width="16" />
			</button>
		{/if}
	</div>
{/snippet}

{#snippet row(t: Task)}
	{@const where = t.status === "active" ? whereItRan(t) : null}
	<li
		class="row"
		data-task-id={t.id}
		tabindex="-1"
		class:closed={isClosed(t)}
		class:dismissed={t.status === "dismissed"}
		class:focused={focused === t.id}
		class:open-detail={expanded === t.id}
		onfocus={() => (focused = t.id)}
	>
		{@render line(t)}
		{#if where}
			<p class="where">{where}</p>
		{/if}
		{#if expanded === t.id}
			{@render detail(t)}
		{/if}
	</li>
{/snippet}

{#snippet detail(t: Task)}
	{@const request = t.assignee === "assistant"}
	{@const project = t.project_id ? projectName.get(t.project_id) : undefined}
	{@const where = whereItRan(t)}
	<div class="detail">
		{#key t.id}
			<TaskComposer
				value={t.notes ?? ""}
				label="Notes"
				placeholder="Notes"
				commitOnBlur
				onCommit={(text) => saveNotes(t, text)}
			/>
		{/key}

		<div class="facts">
			<Popover bind:open={duePickerOpen} placement="bottom-start" offset={4}>
				{#snippet trigger({ toggle: toggleDue })}
					<button type="button" class="fact" onclick={toggleDue} aria-haspopup="dialog">
						{t.due_on ? dueLabel(t.due_on, t.due_at) : "No date"}
					</button>
				{/snippet}
				{#snippet children({ close })}
					<DuePicker
						due_on={t.due_on}
						due_at={t.due_at}
						onPick={(on, at) => {
							saveDue(t, on, at);
							close();
						}}
					/>
				{/snippet}
			</Popover>
			<span class="dot" aria-hidden="true">·</span>
			<Popover bind:open={projectPickerOpen} placement="bottom-start" offset={4}>
				{#snippet trigger({ toggle: toggleProject })}
					<button type="button" class="fact" onclick={toggleProject} aria-haspopup="menu">
						{project ?? "No project"}
					</button>
				{/snippet}
				{#snippet children({ close })}
					<div class="menu" role="menu">
						<button
							type="button"
							role="menuitemradio"
							aria-checked={!t.project_id}
							class="menu-item"
							onclick={() => {
								saveProject(t, null);
								close();
							}}>No project</button
						>
						{#each projectStore.projects as p (p.id)}
							<button
								type="button"
								role="menuitemradio"
								aria-checked={t.project_id === p.id}
								class="menu-item"
								onclick={() => {
									saveProject(t, p.id);
									close();
								}}>{p.name}</button
							>
						{/each}
					</div>
				{/snippet}
			</Popover>
			{#if t.source_url}
				<span class="dot" aria-hidden="true">·</span>
				<button type="button" class="fact" onclick={() => openRoute(t.source_url!)}>
					From {sourceLabel(t.source_url)}
				</button>
			{/if}
		</div>

		{#if request && (where || t.spent_micros)}
			<p class="spend">
				{#if where}Ran on {where}.{/if}
				{#if t.spent_micros}{formatMicrosUSD(t.spent_micros)} so far.{/if}
			</p>
		{/if}

		{#if !isClosed(t)}
			<div class="actions">
				{#if !request}
					<button type="button" class="action" onclick={() => handOver(t)}>Ask the assistant to do it</button>
				{:else if t.status !== "ready"}
					{#if t.status === "open" && !t.next_run_at}
						<button type="button" class="action strong" onclick={() => handOver(t)}>Start</button>
					{/if}
					<button type="button" class="action" onclick={() => takeBack(t)}>Take it back</button>
				{/if}
				<button type="button" class="action" onclick={() => dismiss(t)}>Dismiss</button>
			</div>
			{#if request && t.status !== "ready"}
				<div class="reply">
					<TaskComposer
						label="Add a note for the assistant"
						placeholder="Add a note for the assistant's next run"
						sendLabel="Add note"
						onCommit={(text) => addNote(t, text)}
					/>
				</div>
			{/if}
		{/if}

		{#if history.length > 0}
			<ol class="history" aria-label="History">
				{#each history as e (e.id)}
					<li class:note={e.kind === "note"}>
						<span class="history-line">{eventLine(e)}</span>
						<span class="history-meta">
							{eventTime(e)}
							{#if e.chat_id}
								· <button type="button" class="text-btn" onclick={() => openRoute(`/chat/${e.chat_id}`)}>chat</button>
							{/if}
						</span>
					</li>
				{/each}
			</ol>
		{/if}
	</div>
{/snippet}

<style>
	.add-row {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.5rem 0.25rem;
		border-bottom: 1px solid var(--color-border);
	}
	.add-mark {
		display: inline-flex;
		width: 18px;
		justify-content: center;
		color: var(--color-foreground-subtle);
	}
	.add-input {
		flex: 1;
		min-width: 0;
		background: transparent;
		border: none;
		outline: none;
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}
	.add-input::placeholder {
		color: var(--color-foreground-subtle);
	}
	.add-due {
		flex-shrink: 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.add-hint {
		margin: 0.375rem 0 1.75rem;
		padding-left: 2.15rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.quiet {
		color: var(--color-foreground-muted);
		font-size: 0.875rem;
		padding: 0.5rem 0.25rem;
	}
	.text-btn {
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		font: inherit;
	}

	.section {
		margin-bottom: 2rem;
	}
	.section-label {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		margin: 0 0 0.375rem;
		padding: 0 0.25rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.group-label {
		font-family: var(--font-sans);
		margin: 0.75rem 0 0.125rem;
		padding: 0 0.25rem 0 2.15rem;
		font-size: 0.75rem;
		font-weight: 400;
		color: var(--color-foreground-subtle);
	}
	.fold {
		background: none;
		border: none;
		cursor: pointer;
		margin-left: -0.25rem;
	}

	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.row {
		border-radius: var(--radius-lg);
		outline: none;
	}
	.row.focused:focus,
	.row.focused:focus-within {
		background: color-mix(in srgb, var(--wash-ink) 3%, transparent);
	}
	.row.open-detail {
		background: color-mix(in srgb, var(--wash-ink) 4%, transparent);
	}
	.row.ready {
		padding-bottom: 0.75rem;
		margin-bottom: 0.75rem;
		border-bottom: 1px solid var(--color-border);
	}
	.row.ready:last-child {
		border-bottom: none;
	}

	.line {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.25rem;
		min-height: 2.5rem;
	}
	.line:hover .dismiss {
		opacity: 1;
	}

	.check {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		border: 1.5px solid var(--color-foreground-subtle);
		background: transparent;
		color: var(--color-background);
		cursor: pointer;
		transition: background-color 120ms ease, border-color 120ms ease;
	}
	.check:hover {
		border-color: var(--color-foreground-muted);
	}
	.check.checked {
		background: var(--color-foreground-muted);
		border-color: var(--color-foreground-muted);
	}

	.mark {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		fill: var(--color-foreground-muted);
	}
	.mark.working {
		animation: breathe 1.6s ease-in-out infinite;
	}
	@keyframes breathe {
		50% {
			opacity: 0.35;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.mark.working {
			animation: none;
		}
	}

	.title,
	.title-input {
		flex: 1;
		min-width: 0;
		background: none;
		border: none;
		padding: 0.25rem 0;
		text-align: left;
		color: var(--color-foreground);
		font: inherit;
		font-size: 0.9375rem;
		line-height: 1.4;
	}
	.title {
		cursor: pointer;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.title-input {
		outline: none;
	}
	.closed .title {
		color: var(--color-foreground-muted);
	}
	.row.closed:not(.dismissed) .title {
		text-decoration: line-through;
		text-decoration-color: var(--color-foreground-subtle);
	}

	.meta {
		flex-shrink: 0;
		display: inline-flex;
		gap: 0.75rem;
		background: none;
		border: none;
		padding: 0.25rem 0;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		cursor: pointer;
	}

	.dismiss {
		flex-shrink: 0;
		display: inline-flex;
		background: none;
		border: none;
		padding: 0.25rem;
		border-radius: var(--radius-md);
		color: var(--color-foreground-subtle);
		cursor: pointer;
		opacity: 0;
		transition: opacity 120ms ease;
	}
	.dismiss:hover,
	.dismiss:focus-visible {
		opacity: 1;
		color: var(--color-foreground);
	}
	/* No hover on a phone: the × is always drawn. */
	@media (hover: none) {
		.dismiss {
			opacity: 1;
		}
	}

	.where {
		margin: -0.375rem 0 0.25rem;
		padding-left: 2.15rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.result {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 0.25rem 0.25rem 0.5rem 2.15rem;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 1rem;
		padding-left: 2.15rem;
	}
	.action {
		background: none;
		border: none;
		padding: 0.25rem 0;
		font: inherit;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.action:hover {
		color: var(--color-foreground);
	}
	.action.strong {
		color: var(--color-foreground);
		font-weight: 500;
	}

	.reply {
		margin: 0.25rem 0 0.5rem 2.15rem;
		padding: 0.5rem 0.625rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
	}

	.detail {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 0.25rem 0.5rem 0.875rem 2.15rem;
	}
	.row.ready .detail {
		margin-top: 0.75rem;
	}
	.detail .actions,
	.detail .reply {
		padding-left: 0;
		margin-left: 0;
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.8125rem;
	}
	.fact {
		background: none;
		border: none;
		padding: 0.125rem 0;
		font: inherit;
		color: var(--color-foreground-muted);
		cursor: pointer;
		text-decoration: underline;
		text-decoration-color: transparent;
		text-underline-offset: 3px;
	}
	.fact:hover {
		color: var(--color-foreground);
		text-decoration-color: var(--color-border);
	}
	.dot {
		color: var(--color-foreground-subtle);
	}
	.spend {
		margin: 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.menu {
		display: flex;
		flex-direction: column;
		min-width: 12rem;
		max-height: 18rem;
		overflow-y: auto;
		padding: 0.25rem;
		background: var(--color-background);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
	}
	.menu-item {
		text-align: left;
		background: none;
		border: none;
		border-radius: var(--radius-md);
		padding: 0.375rem 0.5rem;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground);
		cursor: pointer;
	}
	.menu-item:hover,
	.menu-item[aria-checked="true"] {
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
	}

	.history {
		list-style: none;
		margin: 0;
		padding: 0.5rem 0 0;
		border-top: 1px solid var(--color-border);
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.8125rem;
	}
	.history li {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
		color: var(--color-foreground-muted);
	}
	.history li.note .history-line {
		color: var(--color-foreground);
	}
	.history-meta {
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* A phone has no Mod+Return; a to-do goes to the assistant from its detail. */
	@media (max-width: 640px) {
		.add-hint {
			display: none;
		}
		.add-row {
			margin-bottom: 1.75rem;
		}
	}
</style>
