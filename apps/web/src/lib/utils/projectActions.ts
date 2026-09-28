/**
 * "New project" as a door: ask for the name, create it, open it in the window
 * you are in. The Home panel, ⌘K and the projects page all offer it, and a
 * door should behave the same whichever wall it is in.
 */

import { projectStore } from '$lib/stores/project.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { promptText } from '$lib/stores/dialog.svelte';
import { toast } from 'svelte-sonner';

export async function newProject(): Promise<void> {
	const name = (
		await promptText({
			title: 'New project',
			placeholder: 'Name your project',
			confirmLabel: 'Create',
		})
	)?.trim();
	if (!name) return;
	try {
		const project = await projectStore.create(name);
		windowShellStore.openTabFromRoute(`/project/${project.id}`, { label: project.name });
	} catch (e) {
		console.error('[projectActions] Failed to create project:', e);
		toast.error("Your server couldn't create that project", {
			description: 'Nothing changed. Try again',
		});
	}
}

export function openProjects(): void {
	windowShellStore.openTabFromRoute('/projects', { label: 'Projects', focusExisting: true });
}
