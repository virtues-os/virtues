<script lang="ts">
	/**
	 * A project, shown on the thing that is in it: its mark and name, quiet
	 * until approached. The same chip on a chat (desktop corner, phone top
	 * bar) and on a page, so "this is in Kitchen remodel" reads one way
	 * wherever it is said.
	 *
	 * Click does what the host passes; the menu (right-click, or the host's
	 * tap) is the project's: open it, file this elsewhere, take it out.
	 */
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import type { ProjectSummary } from '$lib/api/client';

	interface Props {
		project: Pick<ProjectSummary, 'id' | 'name' | 'icon' | 'accent_color' | 'archived_at'>;
		onclick: (e: MouseEvent) => void;
		oncontextmenu?: (e: MouseEvent) => void;
		/** A title-bar chip: centered, a touch larger, for the phone. */
		bar?: boolean;
		title?: string;
	}

	let { project, onclick, oncontextmenu, bar = false, title }: Props = $props();
</script>

<button
	type="button"
	class="project-chip"
	class:bar
	{title}
	{onclick}
	{oncontextmenu}
>
	<ProjectGlyph icon={project.icon} color={project.accent_color} size={bar ? 15 : 14} />
	<span class="name">{project.name}</span>
	{#if project.archived_at}
		<span class="note">Archived</span>
	{/if}
</button>

<style>
	.project-chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		min-width: 0;
		max-width: 100%;
		height: 28px;
		padding: 0 10px 0 8px;
		border: 0;
		border-radius: 9px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-surface) 72%, transparent);
		backdrop-filter: blur(8px);
		-webkit-backdrop-filter: blur(8px);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition:
			color 0.15s ease,
			background-color 0.15s ease;
	}

	.project-chip:hover {
		color: var(--color-foreground);
		background: var(--color-surface-elevated);
	}

	.project-chip:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	/* On the phone's bar: the title slot, so it reads as where you are. */
	.project-chip.bar {
		height: 32px;
		padding: 0 12px 0 10px;
		font-size: 14px;
		color: var(--color-foreground);
		background: transparent;
	}

	.project-chip.bar:active {
		background: color-mix(in srgb, var(--wash-ink) 8%, transparent);
	}

	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.note {
		flex-shrink: 0;
		color: var(--color-foreground-subtle);
	}
</style>
