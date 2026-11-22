<script lang="ts">
	/**
	 * One task's page: the task whole (TaskDetail) in the page frame. The tab
	 * wears the task's title, and the path names its thread: Threads / Its
	 * thread / The task, where the thread's segment opens it.
	 */
	import type { Tab } from '$lib/tabs/types';
	import type { Task } from '$lib/api/client';
	import TaskDetail from '$lib/components/tasks/TaskDetail.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { usePageCrumbs } from '$lib/stores/paneActions.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { threadCrumb, threadsCrumb } from '$lib/tabs/crumbs';

	let { tab }: { tab: Tab; active?: boolean } = $props();

	const taskId = $derived(tab.route.match(/^\/task\/([^/?#]+)/)?.[1] ?? null);
	let task = $state<Task | null>(null);

	$effect(() => {
		if (task && tab.label !== task.title) windowShellStore.updateTab(tab.id, { label: task.title });
	});

	const crumbThread = $derived(
		task?.thread_id ? [...projectStore.projects, ...projectStore.archived].find((p) => p.id === task?.thread_id) : undefined,
	);
	usePageCrumbs(() =>
		task
			? {
					glyph: 'projects',
					crumbs: [
						threadsCrumb(),
						...(crumbThread ? [threadCrumb(crumbThread.id, crumbThread.name || 'Untitled thread')] : []),
						{ label: task.title },
					],
				}
			: null,
	);
</script>

<div class="task-page">
	<TaskDetail {taskId} bind:current={task} />
</div>

<style>
	.task-page {
		width: 100%;
		height: 100%;
		overflow-y: auto;
	}
</style>
