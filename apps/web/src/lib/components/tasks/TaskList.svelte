<script lang="ts">
	/**
	 * The task list: the owner's to-dos and their requests to the assistant.
	 *
	 *   Ready     requests whose result waits for the owner.
	 *   Working   requests queued or running on the server.
	 *   To-dos    the owner's own, grouped Today, Tomorrow, Later this week,
	 *             Later and No date. A task whose day has passed stays under
	 *             Today and is never marked late.
	 *   Done      folded: accepted requests, finished and dismissed tasks.
	 *
	 * A row is a ring, the title, the icons of what else the task is about,
	 * its home, and its due day. Pointing at a row tells the page (`onHover`),
	 * a click picks it (`onPick`: the Threads door shows it in its preview)
	 * or, where nothing picks, opens the task's page; Enter and a double-click
	 * open the page. Everything else about a task (notes, history, requests'
	 * results and replies) is on its page and in the preview.
	 *
	 * One list, three places. The Threads door shows every task; a thread's
	 * page shows the tasks whose home it is (`projectId`); a thing's page the
	 * tasks under it (`thingId`). A new task opens the New task dialog, with
	 * that home when the list has one. A row leaves out its home's name on
	 * that home's own page. No counts and no red: the list never nags.
	 */
	import type { Task } from "$lib/api/client";
	import { tasksStore } from "$lib/stores/tasks.svelte";
	import { projectStore } from "$lib/stores/project.svelte";
	import { thingsStore } from "$lib/stores/things.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { newTaskDialog } from "$lib/stores/newTask.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import TaskCheck from "$lib/components/tasks/TaskCheck.svelte";
	import { TextAction } from "$lib";
	import { isClosed, toggleTaskDone } from "$lib/tasks/actions";
	import { dueGroup, dueLabel, isoDay, type DueGroup } from "$lib/tasks/due";
	import { homeOf, subjectIcon, subjectName } from "$lib/tasks/homes";
	import { isRequest, requestState } from "$lib/tasks/requests";
	import { toast } from "svelte-sonner";
	import { onMount, tick } from "svelte";

	let {
		active,
		route,
		projectId = null,
		thingId = null,
		readonly = false,
		hideNew = false,
		emptyText = "Nothing here yet.",
		shownId = null,
		pinnedId = null,
		onPick,
		onHover,
	}: {
		active: boolean;
		/** The page's route. `?task=<id>` in it picks that task. */
		route: string;
		/** Only this thread's tasks, and new ones start there. */
		projectId?: string | null;
		/** Only this thing's tasks, and new ones start there. */
		thingId?: string | null;
		/** No New task: an ended thread takes no new tasks. */
		readonly?: boolean;
		/** The page has its own New task button. */
		hideNew?: boolean;
		/** What an empty list says. Null says nothing. */
		emptyText?: string | null;
		/** The task the page is showing, and the one it has kept. */
		shownId?: string | null;
		pinnedId?: string | null;
		onPick?: (t: Task) => void;
		onHover?: (t: Task | null) => void;
	} = $props();

	function inScope(t: Task): boolean {
		if (projectId) return t.thread_id === projectId;
		if (thingId) return t.thing_id === thingId;
		return true;
	}

	/** Past this many to-dos, a scoped list groups them by day too. */
	const GROUP_FROM = 10;
	const POLL_WORKING_MS = 4_000;
	const POLL_IDLE_MS = 30_000;

	const GROUPS: { id: DueGroup; label: string }[] = [
		{ id: "today", label: "Today" },
		{ id: "tomorrow", label: "Tomorrow" },
		{ id: "week", label: "Later this week" },
		{ id: "later", label: "Later" },
		{ id: "none", label: "No date" },
	];

	/** Plain string order: "~" stands for "no date" and sorts last. */
	function cmp(a: string, b: string): number {
		return a < b ? -1 : a > b ? 1 : 0;
	}

	let today = $state(isoDay(new Date()));

	const scoped = $derived(tasksStore.open.filter(inScope));
	const requests = $derived(scoped.filter((t) => t.assignee === "assistant"));
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
		return scoped
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
		const scopedList = !!projectId || !!thingId;
		if (scopedList && todos.length <= GROUP_FROM) return [{ id: "all", label: null as string | null, tasks: todos }];
		const now = new Date();
		return GROUPS.map((g) => ({
			id: g.id as string,
			label: g.label as string | null,
			tasks: todos.filter((t) => dueGroup(t.due_on, now) === g.id),
		})).filter((g) => g.tasks.length > 0);
	});
	const closed = $derived(tasksStore.closed.filter(inScope));
	const isEmpty = $derived(scoped.length === 0);

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

	/** The task's home for its row, left out on that home's own page. */
	function rowHome(t: Task): { icon: string; name: string } | null {
		if (projectId && t.thread_id === projectId) return null;
		if (thingId && t.thing_id === thingId) return null;
		const home = homeOf(t);
		if (!home) return null;
		const name = home.kind === "thread" ? (projectName.get(home.id) ?? subjectName(home)) : subjectName(home);
		return { icon: subjectIcon(home.kind), name };
	}

	function dayText(t: Task): string {
		if (isRequest(t)) return requestState(t);
		if (t.status === "dismissed") return "Dismissed";
		if (isClosed(t)) return "";
		return t.due_on ? dueLabel(t.due_on, t.due_at) : "";
	}

	// ── loading and live refresh ────────────────────────────────────────────

	$effect(() => {
		if (active) void tasksStore.load();
	});

	$effect(() => {
		if (!active) return;
		const every = tasksStore.working ? POLL_WORKING_MS : POLL_IDLE_MS;
		const timer = setInterval(() => {
			if (document.visibilityState === "visible") void tasksStore.refresh();
		}, every);
		const onVisible = () => {
			if (document.visibilityState === "visible") void tasksStore.refresh();
		};
		document.addEventListener("visibilitychange", onVisible);
		return () => {
			clearInterval(timer);
			document.removeEventListener("visibilitychange", onVisible);
		};
	});

	onMount(() => {
		if (!projectStore.loaded) void projectStore.load();
		thingsStore.ensure();
		const timer = setInterval(() => (today = isoDay(new Date())), 60_000);
		return () => clearInterval(timer);
	});

	// ── picking and opening ─────────────────────────────────────────────────

	let focused = $state<string | null>(null);

	function openPage(t: Task) {
		windowShellStore.openTabFromRoute(`/task/${t.id}`, { label: t.title, focusExisting: true });
	}

	function pick(t: Task) {
		focused = t.id;
		if (onPick) onPick(t);
		else openPage(t);
	}

	function newTask() {
		newTaskDialog.open({
			home: projectId ? { kind: "thread", id: projectId } : thingId ? { kind: "thing", id: thingId } : null,
		});
	}

	function rowEl(id: string): HTMLElement | null {
		return document.querySelector<HTMLElement>(`[data-task-id="${CSS.escape(id)}"]`);
	}

	// `?task=<id>` picks that task, once per route.
	const linkedTask = $derived(new URL(route, "http://localhost").searchParams.get("task"));
	let pickedLink: string | null = null;
	$effect(() => {
		const id = linkedTask;
		const linked = id ? tasksStore.find(id) : undefined;
		if (!id || id === pickedLink || !linked || !inScope(linked)) return;
		pickedLink = id;
		focused = id;
		onPick?.(linked);
		void tick().then(() => rowEl(id)?.scrollIntoView({ block: "nearest" }));
	});

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
		const t = tasksStore.find(next);
		if (t) onPick?.(t);
	}

	function onKeydown(e: KeyboardEvent) {
		if (!active || e.defaultPrevented || typing(e.target) || e.altKey || newTaskDialog.isOpen) return;
		const mod = e.metaKey || e.ctrlKey;
		const t = focused ? tasksStore.find(focused) : undefined;
		if (!mod && (e.key === "j" || e.key === "ArrowDown")) {
			e.preventDefault();
			move(1);
		} else if (!mod && (e.key === "k" || e.key === "ArrowUp")) {
			e.preventDefault();
			move(-1);
		} else if (!mod && e.key === "c" && !readonly) {
			e.preventDefault();
			newTask();
		} else if (!t) {
			return;
		} else if (e.key === "Enter") {
			e.preventDefault();
			openPage(t);
		} else if (e.key === "x" && !mod && (t.assignee === "owner" || isClosed(t))) {
			e.preventDefault();
			void toggleTaskDone(t);
		}
	}

	async function toggleDoneFold() {
		doneOpen = !doneOpen;
		if (doneOpen && !tasksStore.closedLoaded) {
			try {
				await tasksStore.loadClosed();
			} catch (e) {
				toast.error("Your server couldn't load finished tasks", {
					description: e instanceof Error ? e.message : undefined,
				});
			}
		}
	}
</script>

<svelte:window onkeydown={onKeydown} />

<div class="task-list" role="presentation" onmouseleave={() => onHover?.(null)}>
	{#if !readonly && !hideNew}
		<div class="new-row">
			<TextAction onclick={newTask}>New task</TextAction>
		</div>
	{/if}

	{#if isEmpty && emptyText}
		<p class="quiet">{emptyText}</p>
	{/if}

	{#if ready.length > 0}
		<h3 class="group-label">Ready for you</h3>
		<ul class="rows">
			{#each ready as t (t.id)}{@render row(t)}{/each}
		</ul>
	{/if}

	{#if working.length > 0}
		<h3 class="group-label">With the assistant</h3>
		<ul class="rows">
			{#each working as t (t.id)}{@render row(t)}{/each}
		</ul>
	{/if}

	{#each todoGroups as group (group.id)}
		{#if group.label}
			<h3 class="group-label">{group.label}</h3>
		{/if}
		<ul class="rows">
			{#each group.tasks as t (t.id)}{@render row(t)}{/each}
		</ul>
	{/each}

	<button type="button" class="fold" aria-expanded={doneOpen} onclick={toggleDoneFold}>
		<Icon icon={doneOpen ? "mynaui:chevron-down" : "mynaui:chevron-right"} width="14" />
		Done
	</button>
	{#if doneOpen}
		{#if closed.length === 0 && tasksStore.closedLoaded}
			<p class="quiet">Accepted requests and finished to-dos show up here.</p>
		{/if}
		<ul class="rows">
			{#each closed as t (t.id)}{@render row(t)}{/each}
		</ul>
	{/if}
</div>

{#snippet row(t: Task)}
	{@const home = rowHome(t)}
	{@const request = isRequest(t)}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<li
		class="row"
		class:closed={isClosed(t)}
		class:shown={t.id === shownId}
		class:pinned={t.id === pinnedId}
		class:focused={t.id === focused}
		data-task-id={t.id}
		tabindex="-1"
		onmouseenter={() => onHover?.(t)}
		onfocus={() => (focused = t.id)}
		onclick={() => pick(t)}
		ondblclick={() => openPage(t)}
	>
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<span class="check" onclick={(e) => e.stopPropagation()} ondblclick={(e) => e.stopPropagation()}>
			{#if request}
				<span class="mark" class:working={t.status === "active"} aria-label="With the assistant" role="img">
					<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"
						><circle cx="8" cy="4" r="2" /><circle cx="4" cy="11" r="2" /><circle cx="12" cy="11" r="2" /></svg
					>
				</span>
			{:else}
				<TaskCheck
					done={t.status === "done"}
					label={isClosed(t) ? `Reopen ${t.title}` : `Complete ${t.title}`}
					onclick={() => void toggleTaskDone(t)}
				/>
			{/if}
		</span>
		<span class="title">{t.title}</span>
		<span class="spacer"></span>
		{#each (t.also_about ?? []).slice(0, 3) as link (link.kind + link.id)}
			<span class="also" title={subjectName(link)}><Icon icon={subjectIcon(link.kind)} width="14" /></span>
		{/each}
		{#if home}
			<span class="home"><Icon icon={home.icon} width="14" /><span class="home-name">{home.name}</span></span>
		{/if}
		<span class="due">{dayText(t)}</span>
	</li>
{/snippet}

<style>
	.task-list {
		display: flex;
		flex-direction: column;
	}
	.new-row {
		margin-bottom: 8px;
	}
	.quiet {
		margin: 8px 0 0;
		font-size: 15px;
		line-height: 24px;
		color: var(--color-foreground-muted);
	}
	.group-label {
		margin: 16px 0 4px;
		font-size: 13px;
		line-height: 16px;
		font-weight: 400;
		color: var(--color-foreground-muted);
	}
	.rows {
		list-style: none;
		margin: 0;
		padding: 0;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		height: 40px;
		padding: 0 12px;
		margin: 0 -12px;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.row:hover,
	.row.shown,
	.row.focused:focus {
		background: var(--hover-bg);
		transition-duration: var(--motion-hover-in);
	}
	.row.pinned {
		background: var(--active-bg);
	}
	.row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}
	.check {
		display: inline-flex;
		flex: none;
	}
	.mark {
		display: inline-flex;
		color: var(--color-foreground-muted);
	}
	.mark svg {
		fill: currentColor;
	}
	.mark.working {
		color: var(--color-primary);
	}
	.title {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 14px;
		color: var(--color-foreground);
	}
	.row.closed .title {
		color: var(--color-foreground-muted);
		text-decoration: line-through;
	}
	.spacer {
		flex: 1 1 auto;
	}
	.also {
		display: inline-flex;
		flex: none;
		color: var(--color-foreground-muted);
	}
	.home {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		flex: none;
		max-width: 200px;
		color: var(--color-foreground-muted);
	}
	.home-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-serif-ui);
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.due {
		flex: none;
		width: 80px;
		text-align: right;
		font-size: 12px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}
	.fold {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		align-self: flex-start;
		margin-top: 16px;
		padding: 0;
		border: 0;
		background: none;
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.fold:hover {
		color: var(--color-foreground);
	}
	.fold:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
		border-radius: var(--radius-sm);
	}
	@media (prefers-reduced-motion: reduce) {
		.row {
			transition: none;
		}
	}
</style>
