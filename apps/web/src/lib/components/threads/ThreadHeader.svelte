<script lang="ts">
	/**
	 * The top of a thread's page: its tile in its color, which opens the icon
	 * and color picker, its name, and its brief, both edited in place. The
	 * brief is what the thread is for, and the assistant follows it in every
	 * chat here. The menu holds Delete; status, Adding and the thing it sits
	 * under are properties in the side column.
	 *
	 * An ended thread says so once, under the header, with Reopen, and its
	 * name and brief are read-only until then.
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
	import { THREAD_ICON } from '$lib/utils/iconHelpers';
	import { deleteProject, openProjects, reopenProject } from '$lib/utils/projectActions';
	import { toast } from 'svelte-sonner';

	let { thread }: { thread: ProjectDetail } = $props();

	let nameDraft = $state('');
	let briefDraft = $state('');
	let nameFocused = $state(false);
	let briefFocused = $state(false);
	$effect(() => {
		if (!nameFocused) nameDraft = thread.name;
		if (!briefFocused) briefDraft = thread.instructions ?? '';
	});

	async function save(patch: { name?: string; instructions?: string | null; icon?: string | null; accent_color?: string | null }) {
		try {
			await projectStore.update(thread.id, patch);
		} catch (e) {
			toast.error("Your server couldn't save that", {
				description: e instanceof Error ? e.message : 'Nothing changed. Try again',
			});
		}
	}

	async function commitName() {
		nameFocused = false;
		const name = nameDraft.trim();
		if (!name) {
			nameDraft = thread.name;
			return;
		}
		if (name !== thread.name) await save({ name });
	}

	async function commitBrief() {
		briefFocused = false;
		const brief = briefDraft.trim() || null;
		if (brief !== (thread.instructions ?? null)) await save({ instructions: brief });
	}

	let iconOpen = $state(false);
	let menuOpen = $state(false);
	const color = $derived(projectColor(thread));
	const dropped = $derived(thread.outcome === 'dropped');
	/** An ended thread is read, not edited, until it is reopened. */
	const ended = $derived(!!thread.archived_at);

	// No confirm: a trip to Recently deleted, with the Undo in the toast. The
	// page it was on goes, so the window lands on the list.
	async function doDelete() {
		if (await deleteProject(thread)) openProjects();
	}

	function day(iso: string): string {
		return new Date(iso).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
	}
</script>

<header class="head">
	<Popover bind:open={iconOpen} placement="bottom-start" offset={4}>
		{#snippet trigger({ toggle }: { toggle: () => void })}
			<button
				type="button"
				class="tile"
				style={`--tile: ${color}`}
				aria-label="Change icon and color"
				data-tip="Icon and color"
				onclick={toggle}
			>
				<ProjectGlyph project={thread} size={22} inherit />
			</button>
		{/snippet}
		{#snippet children({ close }: { close: () => void })}
			<IconPicker
				value={thread.icon ?? null}
				onSelect={(icon) => save({ icon })}
				{close}
				color={thread.accent_color ?? null}
				onColorSelect={(accent_color) => save({ accent_color })}
				label={thread.name}
				fallback={THREAD_ICON}
			/>
		{/snippet}
	</Popover>

	<div class="text">
		<textarea
			class="title"
			bind:value={nameDraft}
			rows="1"
			readonly={ended}
			placeholder="Untitled thread"
			aria-label="Name"
			onfocus={() => (nameFocused = true)}
			onblur={commitName}
			onkeydown={(e) => {
				if (e.key === 'Enter') {
					e.preventDefault();
					e.currentTarget.blur();
				}
				if (e.key === 'Escape') {
					nameDraft = thread.name;
					e.currentTarget.blur();
				}
			}}
		></textarea>
		{#if !ended || thread.instructions}
			<textarea
				class="brief"
				readonly={ended}
				bind:value={briefDraft}
				rows="1"
				placeholder="What is this thread for? The assistant follows it in every chat here."
				aria-label="What this thread is for"
				onfocus={() => (briefFocused = true)}
				onblur={commitBrief}
				onkeydown={(e) => {
					if (e.key === 'Escape') {
						briefDraft = thread.instructions ?? '';
						e.currentTarget.blur();
					}
				}}
			></textarea>
		{/if}
	</div>

	<Popover bind:open={menuOpen} placement="bottom-end" offset={4}>
		{#snippet trigger({ toggle }: { toggle: () => void })}
			<IconButton icon="mynaui:dots" label="More thread actions" expanded={menuOpen} haspopup="menu" onclick={toggle} />
		{/snippet}
		{#snippet children({ close }: { close: () => void })}
			<div class="menu" role="menu">
				<MenuItem
					icon="mynaui:trash"
					label="Delete thread"
					description="It goes to Recently deleted. Its chats, pages and files stay."
					destructive
					onclick={() => {
						close();
						doDelete();
					}}
				/>
			</div>
		{/snippet}
	</Popover>
</header>

{#if thread.archived_at}
	<div class="ended">
		<Icon icon={dropped ? 'mynaui:x-circle' : 'mynaui:check-circle'} width="16" />
		<span>
			You {dropped ? 'dropped' : 'finished'} this thread on {day(thread.archived_at)}. Everything in it stays here.
			Reopen it to add more.
		</span>
		<Button variant="secondary" onclick={() => reopenProject(thread)}>Reopen</Button>
	</div>
{/if}

<style>
	.head {
		display: flex;
		align-items: flex-start;
		gap: 16px;
	}
	/* The thread's color at a sixth behind its glyph, as its tile wears it everywhere. */
	.tile {
		display: grid;
		place-items: center;
		flex: none;
		width: 44px;
		height: 44px;
		padding: 0;
		border: 0;
		border-radius: var(--radius-lg);
		background: color-mix(in srgb, var(--tile) 16%, transparent);
		color: var(--tile);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.tile:hover {
		background: color-mix(in srgb, var(--tile) 24%, transparent);
		transition-duration: var(--motion-hover-in);
	}
	.text {
		display: flex;
		flex-direction: column;
		flex: 1 1 auto;
		min-width: 0;
	}
	.title,
	.brief {
		display: block;
		width: 100%;
		padding: 0;
		border: 0;
		background: transparent;
		resize: none;
		overflow: hidden;
		outline: none;
		field-sizing: content;
	}
	.title {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: var(--text-title);
		line-height: 44px;
		color: var(--color-foreground);
	}
	.brief {
		margin-top: 4px;
		min-height: 24px;
		font: inherit;
		font-size: var(--text-body);
		line-height: 24px;
		color: var(--color-foreground-muted);
	}
	.brief:focus {
		color: var(--color-foreground);
	}
	.title::placeholder,
	.brief::placeholder {
		color: var(--color-foreground-subtle);
	}
	.menu {
		display: flex;
		flex-direction: column;
		min-width: 240px;
		padding: 4px;
	}
	.ended {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 16px;
		border-radius: var(--radius-lg);
		background: var(--fill-bg);
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.ended > :global(svg) {
		flex: none;
		color: var(--color-foreground-subtle);
	}
	.ended > span {
		flex: 1 1 auto;
		min-width: 0;
	}
	@media (max-width: 768px) {
		.head {
			gap: 12px;
		}
		.title {
			font-size: var(--text-heading);
			line-height: 32px;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.tile {
			transition: none;
		}
	}
</style>
