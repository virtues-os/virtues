<script lang="ts">
	/**
	 * New task, on the frame New thread shares: where it will live, a title,
	 * a description, then chips for its home, due day, who it is for and what
	 * else it is about. A date at the end of the title, like "fri" or
	 * "tomorrow 5pm", sets the due day.
	 *
	 * Closing keeps the draft. The next open picks it up again; once it is
	 * empty, the next open starts fresh with the opener's home.
	 */
	import type { TaskLink } from '$lib/api/client';
	import CreateDialog from '$lib/components/create/CreateDialog.svelte';
	import CreateChip from '$lib/components/create/CreateChip.svelte';
	import DuePicker from '$lib/components/tasks/DuePicker.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import HomeMenu from '$lib/components/tasks/HomeMenu.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import RefPicker from '$lib/components/RefPicker.svelte';
	import { newTaskDialog } from '$lib/stores/newTask.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { tasksStore } from '$lib/stores/tasks.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import { dueLabel, parseDue } from '$lib/tasks/due';
	import { homePatch, linkFromUrl, sameLink, subjectIcon, subjectName } from '$lib/tasks/homes';
	import { THREAD_ICON } from '$lib/utils/iconHelpers';
	import { toast } from 'svelte-sonner';
	import { untrack } from 'svelte';

	interface Draft {
		title: string;
		notes: string;
		home: TaskLink | null;
		due: { on: string; at: string | null } | null;
		assignee: 'owner' | 'assistant';
		also: TaskLink[];
	}

	function blank(home: TaskLink | null): Draft {
		return { title: '', notes: '', home, due: null, assignee: 'owner', also: [] };
	}

	let draft = $state<Draft>(blank(null));
	let saving = $state(false);

	// On each open, an empty draft takes the opener's home; one with words
	// in it is kept. Only the opening is tracked, not the draft itself.
	$effect(() => {
		if (!newTaskDialog.isOpen) return;
		untrack(() => {
			if (!draft.title.trim() && !draft.notes.trim()) draft = blank(newTaskDialog.home);
		});
	});

	/** The date typed at the end of the title, when there is one. */
	const typed = $derived(parseDue(draft.title));
	const due = $derived(
		draft.due ?? (typed.due_on ? { on: typed.due_on, at: typed.due_at } : null),
	);
	/** The title without a typed date, unless a date was picked instead. */
	const cleanTitle = $derived((draft.due ? draft.title : typed.title).trim());

	const homeThread = $derived(
		draft.home?.kind === 'thread' ? projectStore.projects.find((p) => p.id === draft.home!.id) : undefined,
	);

	// ── home ─────────────────────────────────────────────────────────────
	let homeOpen = $state(false);

	function pickHome(link: TaskLink | null) {
		draft.home = link;
		draft.also = draft.also.filter((l) => !sameLink(l, link));
		homeOpen = false;
	}

	// ── searches (a person, place or org as home; anything as also-about) ──
	let search = $state<{ for: 'home' | 'also'; at: { x: number; y: number } } | null>(null);
	let homeChipEl = $state<HTMLElement | null>(null);
	let alsoChipEl = $state<HTMLElement | null>(null);

	/** Open a search under its chip, never at the pointer, so the keyboard gets it in the right place too. */
	function searchUnder(el: HTMLElement | null, kind: 'home' | 'also') {
		const r = el?.getBoundingClientRect();
		search = { for: kind, at: { x: r?.left ?? 0, y: (r?.bottom ?? 0) + 4 } };
	}

	function found(entity: { url: string }) {
		const kind = search?.for;
		search = null;
		const link = linkFromUrl(entity.url);
		if (!link) return;
		if (kind === 'home') pickHome(link);
		else if (!draft.also.some((l) => sameLink(l, link)) && !sameLink(link, draft.home)) draft.also = [...draft.also, link];
	}

	// ── create ───────────────────────────────────────────────────────────
	async function create() {
		const title = cleanTitle;
		if (!title || saving) return;
		saving = true;
		try {
			const made = await tasksStore.create({
				title,
				notes: draft.notes.trim() || null,
				assignee: draft.assignee,
				due_on: due?.on ?? null,
				due_at: due?.at ?? null,
				...homePatch(draft.home),
				also_about: draft.also,
			});
			const where = draft.home ? subjectName(draft.home) : null;
			toast(where ? `Task added to ${where}` : 'Task added', {
				action: made
					? {
							label: 'Open',
							onClick: () => windowShellStore.openTabFromRoute(`/task/${made.id}`, { label: made.title, focusExisting: true }),
						}
					: undefined,
			});
			draft = blank(null);
			newTaskDialog.close();
		} catch (e) {
			toast.error("Your server couldn't add that task", {
				description: e instanceof Error ? e.message : 'Try again',
			});
		} finally {
			saving = false;
		}
	}
</script>

<CreateDialog
	open={newTaskDialog.isOpen}
	onClose={() => newTaskDialog.close()}
	label="New task"
	bind:title={draft.title}
	titlePlaceholder="What do you need to do?"
	bind:body={draft.notes}
	bodyPlaceholder="Add a description"
	canCreate={!!cleanTitle}
	{saving}
	createLabel="Create task"
	onCreate={create}
>
	{#snippet crumb()}
		{#if homeThread}
			<ProjectGlyph project={homeThread} size={14} tinted />
		{:else}
			<Icon icon={draft.home ? subjectIcon(draft.home.kind) : 'mynaui:circle'} width="14" />
		{/if}
		<span class="crumb-home">{draft.home ? subjectName(draft.home) : 'No home'}</span>
		<Icon icon="mynaui:chevron-right" width="12" />
		<span>New task</span>
	{/snippet}

	{#snippet chips()}
		<span class="chip-anchor" bind:this={homeChipEl}>
			<CreateChip
				bind:open={homeOpen}
				icon={homeThread ? homeThread.icon || THREAD_ICON : draft.home ? subjectIcon(draft.home.kind) : 'atlas:projects'}
				color={homeThread ? projectColor(homeThread) : null}
				label={draft.home ? subjectName(draft.home) : 'No home'}
				quiet={!draft.home}
			>
				{#snippet menu()}
					<HomeMenu
						value={draft.home}
						onPick={pickHome}
						onSearch={() => {
							homeOpen = false;
							searchUnder(homeChipEl, 'home');
						}}
					/>
				{/snippet}
			</CreateChip>
		</span>

		<CreateChip icon="mynaui:calendar" label={due ? dueLabel(due.on, due.at) : 'No date'} quiet={!due} surface={false}>
			{#snippet menu({ close })}
				<DuePicker
					due_on={due?.on ?? null}
					due_at={due?.at ?? null}
					onPick={(on, at) => {
						draft.due = on ? { on, at } : null;
						close();
					}}
				/>
			{/snippet}
		</CreateChip>

		<CreateChip
			icon={draft.assignee === 'assistant' ? 'mynaui:sparkles' : 'mynaui:user'}
			label={draft.assignee === 'assistant' ? 'For the assistant' : 'For you'}
		>
			{#snippet menu({ close })}
				<MenuItem
					role="menuitemradio"
					icon="mynaui:user"
					checked={draft.assignee === 'owner'}
					label="You"
					onclick={() => {
						draft.assignee = 'owner';
						close();
					}}
				/>
				<MenuItem
					role="menuitemradio"
					icon="mynaui:sparkles"
					checked={draft.assignee === 'assistant'}
					label="The assistant"
					description="It works on the task in the background and brings you the result."
					onclick={() => {
						draft.assignee = 'assistant';
						close();
					}}
				/>
			{/snippet}
		</CreateChip>

		<span class="chip-anchor" bind:this={alsoChipEl}>
			{#if draft.also.length === 0}
				<CreateChip icon="mynaui:link" label="Also about" quiet onclick={() => searchUnder(alsoChipEl, 'also')} />
			{:else}
				<CreateChip icon="mynaui:link" label={`Also about ${draft.also.map((l) => subjectName(l)).join(', ')}`}>
					{#snippet menu({ close })}
						{#each draft.also as link (link.kind + link.id)}
							<MenuItem
								role="menuitemcheckbox"
								icon={subjectIcon(link.kind)}
								checked
								label={subjectName(link)}
								onclick={() => (draft.also = draft.also.filter((l) => !sameLink(l, link)))}
							/>
						{/each}
						<MenuItem
							icon="mynaui:plus"
							label="Add…"
							onclick={() => {
								close();
								searchUnder(alsoChipEl, 'also');
							}}
						/>
					{/snippet}
				</CreateChip>
			{/if}
		</span>

		<!-- Inside the dialog, so it stacks over it as the chips' pickers do. -->
		{#if search}
			<RefPicker
				mode="single"
				position={search.at}
				placeholder={search.for === 'home' ? 'Find a person, place or organization…' : 'Find a thread, person, place, organization or thing…'}
				filter={(e) => {
					const link = linkFromUrl(e.url);
					if (!link) return false;
					return search?.for === 'home' ? link.kind === 'person' || link.kind === 'place' || link.kind === 'org' : true;
				}}
				onSelect={found}
				onClose={() => (search = null)}
			/>
		{/if}
	{/snippet}
</CreateDialog>

<style>
	.crumb-home {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-foreground);
	}
	.chip-anchor {
		display: inline-flex;
	}
</style>
