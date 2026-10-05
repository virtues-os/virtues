<script lang="ts">
	/**
	 * A project's header: its mark and color, its name, its brief, the way
	 * into a new chat, and the project's own verbs. Always live, no edit mode.
	 *
	 * The subtitle is the *brief* (`instructions`): a standing direction the
	 * assistant follows in every chat in this project. No auto-generated
	 * summary: the chats and material are directly below and fully legible,
	 * so a generated description would only restate what's visible. What
	 * can't be derived is what the project is *for*.
	 */
	import type { ProjectDetail } from '$lib/api/client';
	import Icon from '$lib/components/Icon.svelte';
	import IconPicker from '$lib/components/IconPicker.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import { Button, IconButton } from '$lib';
	import { Popover } from '$lib/floating';
	import { projectStore } from '$lib/stores/project.svelte';
	import { projectColor } from '$lib/sidebar/pin-colors';
	import {
		archiveProject,
		deleteProject,
		newChatInProject,
		openProjects,
		unarchiveProject
	} from '$lib/utils/projectActions';

	let { project }: { project: ProjectDetail } = $props();

	let nameDraft = $state('');
	let briefDraft = $state('');
	let nameFocused = $state(false);
	let briefFocused = $state(false);
	$effect(() => {
		if (!nameFocused) nameDraft = project.name;
		if (!briefFocused) briefDraft = project.instructions ?? '';
	});

	async function commitName() {
		nameFocused = false;
		const name = nameDraft.trim();
		if (!name) {
			nameDraft = project.name;
			return;
		}
		if (name === project.name) return;
		await projectStore.update(project.id, { name });
	}

	async function commitBrief() {
		briefFocused = false;
		const brief = briefDraft.trim() || null;
		if (brief === (project.instructions ?? null)) return;
		await projectStore.update(project.id, { instructions: brief });
	}

	let iconOpen = $state(false);
	let overflowOpen = $state(false);

	/**
	 * Icon and color share one picker. `accent_color` predates the token rule
	 * and still holds raw hex for projects colored before it; writing a token
	 * key here converts a project the first time it's recolored, the only
	 * migration that doesn't guess on the owner's behalf.
	 */
	async function setIcon(icon: string | null) {
		await projectStore.update(project.id, { icon });
	}
	async function setAccent(color: string | null) {
		await projectStore.update(project.id, { accent_color: color });
	}

	// Reversible, so no confirm. The project stays open in this tab, marked.
	async function toggleArchive() {
		if (project.archived_at) await unarchiveProject(project);
		else await archiveProject(project);
	}

	// No confirm: a trip to Recently deleted, with the Undo in the toast. The
	// page it was on goes, so the window lands on the list.
	async function doDelete() {
		if (await deleteProject(project)) openProjects();
	}
</script>

<header class="head">
	<div class="head-main">
		<Popover bind:open={iconOpen} placement="bottom-start">
			{#snippet trigger({ toggle }: { toggle: () => void })}
				<button
					class="project-icon"
					style={`--room-accent: ${projectColor(project)}`}
					title="Change icon and color"
					aria-label="Change icon and color"
					onclick={toggle}
				>
					<ProjectGlyph {project} size={22} inherit />
				</button>
			{/snippet}
			{#snippet children({ close }: { close: () => void })}
				<IconPicker
					value={project.icon ?? null}
					onSelect={setIcon}
					{close}
					color={project.accent_color ?? null}
					onColorSelect={setAccent}
				/>
			{/snippet}
		</Popover>

		<div class="head-text">
			<textarea
				class="title-input font-serif"
				bind:value={nameDraft}
				rows="1"
				placeholder="Untitled project"
				onfocus={() => (nameFocused = true)}
				onblur={commitName}
				onkeydown={(e) => {
					if (e.key === 'Enter') {
						e.preventDefault();
						e.currentTarget.blur();
					}
					if (e.key === 'Escape') {
						nameDraft = project.name;
						e.currentTarget.blur();
					}
				}}
			></textarea>
			<textarea
				class="desc-input"
				bind:value={briefDraft}
				rows="1"
				placeholder="What this project is for. The assistant follows this in every chat here."
				onfocus={() => (briefFocused = true)}
				onblur={commitBrief}
				onkeydown={(e) => {
					if (e.key === 'Escape') {
						briefDraft = project.instructions ?? '';
						e.currentTarget.blur();
					}
				}}
			></textarea>
		</div>

		<div class="head-actions">
			<Button variant="secondary" size="sm" icon="ri:chat-new-line" onclick={() => newChatInProject(project)}
				>New chat</Button
			>
			<Popover bind:open={overflowOpen} placement="bottom-end">
				{#snippet trigger({ toggle }: { toggle: () => void })}
					<IconButton
						icon="ri:more-line"
						label="More project actions"
						expanded={overflowOpen}
						haspopup="menu"
						onclick={toggle}
					/>
				{/snippet}
				{#snippet children({ close }: { close: () => void })}
					<div class="menu">
						<MenuItem
							icon={project.archived_at ? 'ri:inbox-unarchive-line' : 'ri:archive-line'}
							label={project.archived_at ? 'Unarchive project' : 'Archive project'}
							onclick={() => {
								close();
								toggleArchive();
							}}
						/>
						<MenuItem
							icon="ri:delete-bin-line"
							label="Delete project"
							destructive
							onclick={() => {
								close();
								doDelete();
							}}
						/>
					</div>
				{/snippet}
			</Popover>
		</div>
	</div>
</header>

{#if project.archived_at}
	<!-- Archived is a state of the whole page, so it is said once, above
	     the content, with the way back beside it. -->
	<div class="archived-note">
		<Icon icon="ri:archive-line" width="15" />
		<span>
			You archived this project on {new Date(project.archived_at).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}.
			Its chats and items are all here. Unarchive it to add more.
		</span>
		<Button variant="secondary" size="sm" onclick={toggleArchive}>Unarchive</Button>
	</div>
{/if}

<style>
	.head { display: flex; flex-direction: column; gap: 0.7rem; }
	.head-main { display: flex; align-items: flex-start; gap: 14px; }
	.head-text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
	.head-actions { display: flex; gap: 2px; flex-shrink: 0; }

	/* The same tinted chip the projects list draws, so a project looks like
	   itself on its own page. */
	.project-icon {
		display: grid; place-items: center; width: 46px; height: 46px; flex-shrink: 0;
		border-radius: 12px; cursor: pointer;
		background: color-mix(in srgb, var(--room-accent) 16%, transparent);
		border: 1px solid color-mix(in srgb, var(--room-accent) 30%, var(--color-border));
		color: color-mix(in srgb, var(--room-accent) 78%, var(--color-foreground));
		transition: border-color 120ms ease;
	}
	.project-icon:hover { border-color: color-mix(in srgb, var(--room-accent) 60%, var(--color-border)); }

	.title-input, .desc-input {
		display: block; width: 100%; resize: none; overflow: hidden;
		border: none; background: transparent; outline: none;
		field-sizing: content;
	}
	.title-input {
		font-size: 2rem; font-weight: 500; line-height: 1.12;
		color: var(--color-foreground); padding: 0 0 2px;
		border-bottom: 1.5px solid transparent;
	}
	.title-input:focus {
		border-bottom-color: color-mix(in srgb, var(--color-foreground) 45%, var(--color-border));
	}
	.desc-input {
		margin-top: 0.4rem; min-height: 1.5rem; padding: 0;
		font: inherit; font-size: 0.95rem; line-height: 1.55;
		color: var(--color-foreground-muted);
	}
	.desc-input:focus { color: var(--color-foreground); }
	.title-input::placeholder, .desc-input::placeholder { color: var(--color-foreground-subtle); }

	.menu { display: flex; flex-direction: column; min-width: 190px; padding: 4px; }

	.archived-note {
		display: flex; align-items: center; gap: 10px;
		padding: 10px 12px; border-radius: 10px;
		background: var(--color-surface-elevated);
		font-size: 0.85rem; line-height: 1.45; color: var(--color-foreground-muted);
	}
	.archived-note > :global(svg) { flex-shrink: 0; color: var(--color-foreground-subtle); }
	.archived-note > span { flex: 1; min-width: 0; }

	/* A phone's width can't hold the icon, the text and the actions in one
	   row: the text column collapsed to a letter wide. The actions take their
	   own line under the text. */
	@media (max-width: 768px) {
		.head-main {
			flex-wrap: wrap;
			gap: 12px;
		}
		/* 44 + 12: the actions line up under the text, not the icon. */
		.project-icon {
			width: 44px;
			height: 44px;
		}
		.head-text {
			flex-basis: calc(100% - 56px);
		}
		.head-actions {
			width: 100%;
			padding-left: 56px;
			gap: 6px;
		}
		.title-input {
			font-size: 1.6rem;
		}
	}
</style>
