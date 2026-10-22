<script lang="ts">
	/**
	 * Tasks: one to-do list the owner and the assistant share
	 * (agents/plan/tasks-plan.md). Shaped like Reminders, not a data grid:
	 * an add row, open tasks grouped by when they are due, Done folded at
	 * the bottom. A row is a checkbox, a title and at most three chips, and
	 * opens in place to edit.
	 *
	 * No counts anywhere, the tab's description included: the list must
	 * never nag, and a number is the first step toward that.
	 */
	import type { Tab } from "$lib/tabs/types";
	import { listTaskEvents, type Task, type TaskEvent, type TaskPatch } from "$lib/api/client";
	import { askVirtues } from "$lib/stores/pendingPrompt.svelte";
	import { tasksStore } from "$lib/stores/tasks.svelte";
	import { projectStore } from "$lib/stores/project.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { Page } from "$lib";
	import SubNav, { type SubNavItem } from "$lib/components/SubNav.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { dueGroup, dueLabel, isoDay, parseDue, type DueGroup } from "$lib/tasks/due";
	import { toast } from "svelte-sonner";
	import { onMount, tick } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Filter = "mine" | "assistant" | "all";
	const filter = $derived<Filter>(
		(tab.route.match(/^\/tasks\/(assistant|all)$/)?.[1] as Filter) ?? "mine",
	);
	const filters: SubNavItem[] = [
		{ id: "mine", label: "Mine" },
		{ id: "assistant", label: "Assistant" },
		{ id: "all", label: "All" },
	];

	const GROUPS: { id: DueGroup; label: string }[] = [
		{ id: "overdue", label: "Overdue" },
		{ id: "today", label: "Today" },
		{ id: "upcoming", label: "Upcoming" },
		{ id: "none", label: "No date" },
	];

	function shown(t: Task): boolean {
		if (filter === "mine") return t.assignee === "owner";
		if (filter === "assistant") return t.assignee === "assistant";
		return true;
	}

	/** The day rolls over while the view is open; regroup when it does. */
	let today = $state(isoDay(new Date()));

	const grouped = $derived.by(() => {
		void today;
		const now = new Date();
		const byGroup = new Map<DueGroup, Task[]>(GROUPS.map((g) => [g.id, []]));
		for (const t of tasksStore.open.filter(shown)) byGroup.get(dueGroup(t.due_on, now))!.push(t);
		for (const list of byGroup.values()) {
			list.sort(
				(a, b) =>
					(a.due_on ?? "").localeCompare(b.due_on ?? "") ||
					(a.due_at ?? "").localeCompare(b.due_at ?? "") ||
					a.sort_order - b.sort_order ||
					b.created_at.localeCompare(a.created_at),
			);
		}
		return GROUPS.map((g) => ({ ...g, tasks: byGroup.get(g.id)! })).filter((g) => g.tasks.length > 0);
	});
	const closed = $derived(tasksStore.closed.filter(shown));
	const isEmpty = $derived(grouped.length === 0);

	const projectName = $derived.by(() => {
		const names = new Map<string, string>();
		for (const p of [...projectStore.projects, ...projectStore.archived]) names.set(p.id, p.name);
		return names;
	});

	// The assistant changes tasks from chats, so the list reloads whenever
	// its tab comes forward, not only when it first opens.
	$effect(() => {
		if (active) void tasksStore.load();
	});

	onMount(() => {
		if (!projectStore.loaded) void projectStore.load();
		const timer = setInterval(() => (today = isoDay(new Date())), 60_000);
		return () => clearInterval(timer);
	});

	// ── adding ──────────────────────────────────────────────────────────────

	let draft = $state("");
	let addInput: HTMLInputElement | undefined = $state();
	const preview = $derived(parseDue(draft));

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
	 * next task can be typed straight away; a refused task comes back into
	 * the row if nothing new has been typed there.
	 */
	async function add() {
		const { title, due_on, due_at } = preview;
		if (!title) return;
		const typed = draft;
		draft = "";
		try {
			await tasksStore.create({
				title,
				due_on,
				due_at,
				assignee: filter === "assistant" ? "assistant" : "owner",
			});
		} catch (e) {
			if (!draft) draft = typed;
			toast.error("Your server couldn't add that task", { description: errorText(e) });
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

	function toggleDone(t: Task) {
		const closing = t.status === "open" || t.status === "active";
		void save(t.id, { status: closing ? "done" : "open" }, "Your server couldn't update that task");
	}

	function dismiss(t: Task) {
		if (expanded === t.id) expanded = null;
		void save(t.id, { status: "dismissed" }, "Your server couldn't dismiss that task");
		toast("Task dismissed", {
			action: { label: "Undo", onClick: () => void save(t.id, { status: "open" }, "Your server couldn't restore that task") },
		});
	}

	function errorText(e: unknown): string | undefined {
		return e instanceof Error ? e.message : undefined;
	}

	// ── done fold ───────────────────────────────────────────────────────────

	let doneOpen = $state(false);

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

	// ── editing in place ────────────────────────────────────────────────────

	let expanded = $state<string | null>(null);

	let history = $state<TaskEvent[]>([]);

	function toggleExpanded(t: Task) {
		expanded = expanded === t.id ? null : t.id;
		history = [];
		if (expanded) void loadHistory(t.id);
	}

	// `?task=<id>` opens that task, as a Recent row in the sidebar does. Read
	// once per route, so closing the task afterwards keeps it closed.
	const linkedTask = $derived(new URL(tab.route, "http://localhost").searchParams.get("task"));
	let openedLink: string | null = null;
	$effect(() => {
		const id = linkedTask;
		if (!id || id === openedLink || !tasksStore.find(id)) return;
		openedLink = id;
		expanded = id;
		history = [];
		void loadHistory(id);
		void tick().then(() =>
			document.querySelector(`[data-task-id="${CSS.escape(id)}"]`)?.scrollIntoView({ block: "nearest" }),
		);
	});

	async function loadHistory(id: string) {
		try {
			const events = await listTaskEvents(id);
			if (expanded === id) history = events;
		} catch (e) {
			toast.error("Your server couldn't load this task's history", { description: errorText(e) });
		}
	}

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
					done: "finished it",
					dismissed: "dismissed it",
					active: "started on it",
					open: "reopened it",
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

	function openChat(chatId: string) {
		windowShellStore.openTabFromRoute(`/chat/${chatId}`, { focusExisting: true });
	}

	/**
	 * Give the task to the assistant and start it on the work: a new chat,
	 * every time, so nothing from an older conversation rides along. The
	 * history links whichever chat does the work.
	 */
	async function handToAssistant(t: Task) {
		try {
			await tasksStore.update(t.id, { assignee: "assistant" });
		} catch (e) {
			toast.error("Your server couldn't hand that task over", { description: errorText(e) });
			return;
		}
		const notes = t.notes ? `\n\nNotes: ${t.notes}` : "";
		askVirtues(`Please take this task (${t.id}): ${t.title}${notes}`, t.project_id);
	}

	function takeBack(t: Task) {
		void save(t.id, { assignee: "owner" }, "Your server couldn't take that task back");
		void loadHistory(t.id);
	}

	function saveTitle(t: Task, value: string) {
		const title = value.trim();
		if (title && title !== t.title) void save(t.id, { title }, "Your server couldn't rename that task");
	}

	function saveNotes(t: Task, value: string) {
		const notes = value.trim() || null;
		if (notes !== (t.notes ?? null)) void save(t.id, { notes }, "Your server couldn't save those notes");
	}

	/** The time input's value for a task's local due time, or "". */
	function timeValue(t: Task): string {
		if (!t.due_at) return "";
		const d = new Date(t.due_at);
		return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
	}

	function saveDue(t: Task, day: string, time: string) {
		const due_on = day || null;
		let due_at: string | null = null;
		if (due_on && time) {
			const [y, m, d] = due_on.split("-").map(Number);
			const [h, min] = time.split(":").map(Number);
			due_at = new Date(y, m - 1, d, h, min).toISOString();
		}
		if (due_on === t.due_on && due_at === t.due_at) return;
		void save(t.id, { due_on, due_at }, "Your server couldn't change that date");
	}

	function saveProject(t: Task, value: string) {
		const project_id = value || null;
		if (project_id !== t.project_id) void save(t.id, { project_id }, "Your server couldn't move that task");
	}

	function openSource(url: string) {
		windowShellStore.openTabFromRoute(url, { focusExisting: true });
	}

	function sourceLabel(url: string): string {
		const kind = url.split("/")[1];
		return { page: "Page", chat: "Chat", project: "Project", day: "Day" }[kind] ?? "Source";
	}
</script>

<div class="flex h-full w-full flex-col bg-background">
	<SubNav
		tabId={tab.id}
		route={tab.route}
		base="/tasks"
		default="mine"
		items={filters}
		divider
		ariaLabel="Whose tasks"
	/>

	<div class="min-h-0 flex-1">
		<Page title="Tasks" maxWidth="prose">
			<form
				class="add-row"
				onsubmit={(e) => {
					e.preventDefault();
					void add();
				}}
			>
				<span class="add-mark" aria-hidden="true"><Icon icon="ri:add-line" width="16" /></span>
				<input
					bind:this={addInput}
					bind:value={draft}
					class="add-input"
					placeholder={filter === "assistant" ? "A task for the assistant" : "New task"}
					aria-label="New task"
				/>
				{#if preview.due_on && preview.title}
					<span class="chip chip-due" title="Read from what you typed">
						{dueLabel(preview.due_on, preview.due_at)}
					</span>
				{/if}
			</form>

			{#if tasksStore.loading && tasksStore.open.length === 0}
				<p class="quiet">Loading tasks…</p>
			{:else if tasksStore.error}
				<p class="quiet">
					Your server couldn't load your tasks.
					<button type="button" class="link" onclick={() => tasksStore.load()}>Try again</button>
				</p>
			{:else if isEmpty}
				<p class="quiet">
					{#if filter === "assistant"}
						Tasks you hand to the assistant show up here.
					{:else}
						Nothing to do. Type above to add a task; a date at the end, like "fri" or "tomorrow 5pm", sets when it's due.
					{/if}
				</p>
			{/if}

			{#each grouped as group (group.id)}
				<section class="group" aria-label={group.label}>
					<h2 class="group-label" class:overdue={group.id === "overdue"}>{group.label}</h2>
					<ul class="rows">
						{#each group.tasks as t (t.id)}
							{@render row(t)}
						{/each}
					</ul>
				</section>
			{/each}

			<section class="group done-group">
				<button type="button" class="group-label fold" aria-expanded={doneOpen} onclick={toggleDoneFold}>
					<Icon icon={doneOpen ? "ri:arrow-down-s-line" : "ri:arrow-right-s-line"} width="14" />
					Done
				</button>
				{#if doneOpen}
					{#if closed.length === 0 && tasksStore.closedLoaded}
						<p class="quiet">Finished and dismissed tasks show up here.</p>
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

{#snippet row(t: Task)}
	{@const isClosed = t.status === "done" || t.status === "dismissed"}
	{@const project = t.project_id ? projectName.get(t.project_id) : undefined}
	<li class="row" data-task-id={t.id} class:closed={isClosed} class:dismissed={t.status === "dismissed"} class:open-detail={expanded === t.id}>
		<div class="row-line">
			<button
				type="button"
				class="check"
				class:checked={t.status === "done"}
				role="checkbox"
				aria-checked={t.status === "done"}
				aria-label={isClosed ? `Reopen ${t.title}` : `Complete ${t.title}`}
				onclick={() => toggleDone(t)}
			>
				{#if t.status === "done"}
					<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"
						><path d="M4 8.3 6.7 11 12 5.2" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" /></svg
					>
				{/if}
			</button>
			<button type="button" class="row-main" aria-expanded={expanded === t.id} onclick={() => toggleExpanded(t)}>
				{#if t.status === "active"}
					<span class="active-dot" title="The assistant is working on this"></span>
				{/if}
				<span class="title">{t.title}</span>
				<span class="chips">
					{#if t.status === "dismissed"}
						<span class="chip">Dismissed</span>
					{/if}
					{#if t.due_on && !isClosed}
						<span class="chip chip-due" class:overdue={dueGroup(t.due_on) === "overdue"}>{dueLabel(t.due_on, t.due_at)}</span>
					{/if}
					{#if project}
						<span class="chip">{project}</span>
					{/if}
					{#if t.assignee === "assistant" && filter !== "assistant"}
						<span class="chip chip-assistant" title="Assigned to the assistant">Assistant</span>
					{/if}
				</span>
			</button>
			{#if !isClosed}
				<button type="button" class="dismiss" aria-label={`Dismiss ${t.title}`} title="Dismiss" onclick={() => dismiss(t)}>
					<Icon icon="ri:close-line" width="16" />
				</button>
			{/if}
		</div>

		{#if expanded === t.id}
			<div class="detail">
				<input
					class="detail-title"
					value={t.title}
					aria-label="Title"
					onblur={(e) => saveTitle(t, e.currentTarget.value)}
					onkeydown={(e) => e.key === "Enter" && e.currentTarget.blur()}
				/>
				<textarea
					class="detail-notes"
					value={t.notes ?? ""}
					placeholder="Notes"
					aria-label="Notes"
					rows="3"
					onblur={(e) => saveNotes(t, e.currentTarget.value)}
				></textarea>
				<div class="facts">
					<label class="fact">
						<span>Due</span>
						<input
							type="date"
							value={t.due_on ?? ""}
							onchange={(e) => saveDue(t, e.currentTarget.value, timeValue(t))}
						/>
					</label>
					{#if t.due_on}
						<label class="fact">
							<span>Time</span>
							<input
								type="time"
								value={timeValue(t)}
								onchange={(e) => saveDue(t, t.due_on ?? "", e.currentTarget.value)}
							/>
						</label>
					{/if}
					<label class="fact">
						<span>Project</span>
						<select value={t.project_id ?? ""} onchange={(e) => saveProject(t, e.currentTarget.value)}>
							<option value="">None</option>
							{#each projectStore.projects as p (p.id)}
								<option value={p.id}>{p.name}</option>
							{/each}
						</select>
					</label>
					{#if t.source_url}
						<button type="button" class="fact link" onclick={() => openSource(t.source_url!)}>
							<Icon icon="ri:link" width="14" />
							From a {sourceLabel(t.source_url).toLowerCase()}
						</button>
					{/if}
				</div>
				{#if !isClosed}
					<div class="hand">
						{#if t.assignee === "owner"}
							<button type="button" class="hand-btn" onclick={() => handToAssistant(t)}>
								<Icon icon="ri:send-plane-line" width="14" />
								Hand to assistant
							</button>
						{:else}
							<button type="button" class="hand-btn" onclick={() => takeBack(t)}>Take back</button>
						{/if}
					</div>
				{/if}
				{#if history.length > 0}
					<ol class="history" aria-label="History">
						{#each history as e (e.id)}
							<li class:note={e.kind === "note"}>
								<span class="history-line">{eventLine(e)}</span>
								<span class="history-meta">
									{eventTime(e)}
									{#if e.chat_id}
										· <button type="button" class="link" onclick={() => openChat(e.chat_id!)}>chat</button>
									{/if}
								</span>
							</li>
						{/each}
					</ol>
				{/if}
			</div>
		{/if}
	</li>
{/snippet}

<style>
	.add-row {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.5rem 0.25rem;
		border-bottom: 1px solid var(--color-border);
		margin-bottom: 1rem;
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

	.quiet {
		color: var(--color-foreground-muted);
		font-size: 0.875rem;
		padding: 0.5rem 0.25rem;
	}
	.link {
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		font: inherit;
	}

	.group {
		margin-bottom: 1.25rem;
	}
	.group-label {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		font-size: 0.75rem;
		font-weight: 600;
		letter-spacing: 0.02em;
		color: var(--color-foreground-muted);
		padding: 0 0.25rem 0.25rem;
	}
	.group-label.overdue {
		color: var(--color-error);
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
	}
	.row.open-detail {
		background: color-mix(in srgb, var(--wash-ink) 4%, transparent);
	}
	.row-line {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.25rem 0.25rem;
		min-height: 2.25rem;
	}
	.row-line:hover .dismiss {
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

	.row-main {
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		background: none;
		border: none;
		padding: 0.25rem 0;
		text-align: left;
		cursor: pointer;
		color: var(--color-foreground);
		font-size: 0.9375rem;
	}
	.title {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.closed .title {
		color: var(--color-foreground-muted);
	}
	.row.closed:not(.dismissed) .title {
		text-decoration: line-through;
		text-decoration-color: var(--color-foreground-subtle);
	}
	.active-dot {
		flex-shrink: 0;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-primary);
	}

	.chips {
		display: inline-flex;
		gap: 0.25rem;
		flex-shrink: 0;
		margin-left: auto;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		padding: 0.0625rem 0.5rem;
		font-size: 0.75rem;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, var(--wash-ink) 7%, transparent);
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}
	.chip-due.overdue {
		color: var(--color-error);
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

	.detail {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		padding: 0.25rem 0.5rem 0.75rem 2.15rem;
	}
	.detail-title,
	.detail-notes {
		width: 100%;
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: 0.375rem 0.5rem;
		color: var(--color-foreground);
		font: inherit;
		font-size: 0.875rem;
	}
	.detail-notes {
		resize: vertical;
	}
	.facts {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		align-items: center;
	}
	.fact {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
	.hand {
		display: flex;
		gap: 0.5rem;
	}
	.hand-btn {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		padding: 0.25rem 0.625rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 0.8125rem;
		cursor: pointer;
	}
	.hand-btn:hover {
		background: color-mix(in srgb, var(--wash-ink) 5%, transparent);
	}
	.history {
		list-style: none;
		margin: 0.25rem 0 0;
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
	.fact input,
	.fact select {
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: 0.125rem 0.375rem;
		color: var(--color-foreground);
		font: inherit;
	}
</style>
