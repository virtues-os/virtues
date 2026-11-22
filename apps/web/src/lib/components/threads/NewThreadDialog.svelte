<script lang="ts">
	/**
	 * New thread, on the frame New task shares: its glyph and color, its
	 * name, why you are following it, then chips for where it stands (active
	 * or backlog), how things get filed in it, and the thing it sits under.
	 *
	 * Closing keeps the draft until it is empty or the thread is made.
	 */
	import CreateDialog from '$lib/components/create/CreateDialog.svelte';
	import CreateChip from '$lib/components/create/CreateChip.svelte';
	import Icon from '$lib/components/Icon.svelte';
	import IconPicker from '$lib/components/IconPicker.svelte';
	import MenuItem from '$lib/components/MenuItem.svelte';
	import ProjectGlyph from '$lib/components/ProjectGlyph.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import { newThreadDialog } from '$lib/stores/newThread.svelte';
	import { projectStore } from '$lib/stores/project.svelte';
	import { thingsStore } from '$lib/stores/things.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { accentCss } from '$lib/sidebar/pin-colors';
	import { THREAD_ICON } from '$lib/utils/iconHelpers';
	import { toast } from 'svelte-sonner';
	import { untrack } from 'svelte';

	interface Draft {
		name: string;
		why: string;
		icon: string | null;
		color: string | null;
		backlog: boolean;
		automatic: boolean;
		under: string | null;
	}

	function blank(under: string | null): Draft {
		return { name: '', why: '', icon: null, color: null, backlog: false, automatic: false, under };
	}

	let draft = $state<Draft>(blank(null));
	let saving = $state(false);
	let more = $state(false);
	let glyphOpen = $state(false);
	let dialog = $state<ReturnType<typeof CreateDialog> | null>(null);

	// On each open, an empty draft takes the opener's thing; one with words
	// in it is kept. Only the opening is tracked, not the draft itself.
	$effect(() => {
		if (!newThreadDialog.isOpen) return;
		untrack(() => {
			if (!draft.name.trim() && !draft.why.trim()) draft = blank(newThreadDialog.under);
		});
	});

	/** The glyph as ProjectGlyph draws it, from the draft. */
	const glyph = $derived({ id: 'draft', icon: draft.icon, accent_color: draft.color });
	const underThing = $derived(draft.under ? thingsStore.byId(draft.under) : undefined);

	async function create() {
		const name = draft.name.trim();
		if (!name || saving) return;
		saving = true;
		try {
			const made = await projectStore.create(name, {
				icon: draft.icon,
				accent_color: draft.color,
				thing_id: draft.under,
				instructions: draft.why.trim() || null,
				is_backlog: draft.backlog,
				is_adding_automatic: draft.automatic,
			});
			const show = newThreadDialog.onOpen ?? ((route: string, label: string) => windowShellStore.openTabFromRoute(route, { label }));
			if (more) {
				toast(`Thread created: ${made.name}`, {
					action: { label: 'Open', onClick: () => show(`/thread/${made.id}`, made.name) },
				});
				draft = { ...draft, name: '', why: '', icon: null, color: null };
				dialog?.focusTitle();
			} else {
				draft = blank(null);
				newThreadDialog.close();
				show(`/thread/${made.id}`, made.name);
			}
		} catch (e) {
			toast.error("Your server couldn't create that thread", {
				description: e instanceof Error ? e.message : 'Nothing changed. Try again',
			});
		} finally {
			saving = false;
		}
	}
</script>

<CreateDialog
	bind:this={dialog}
	open={newThreadDialog.isOpen}
	onClose={() => newThreadDialog.close()}
	label="New thread"
	bind:title={draft.name}
	titlePlaceholder="What are you following?"
	bind:body={draft.why}
	bodyPlaceholder="Why are you following it?"
	bind:more
	canCreate={!!draft.name.trim()}
	{saving}
	createLabel="Create thread"
	onCreate={create}
>
	{#snippet crumb()}
		<Icon icon={underThing ? 'atlas:things' : THREAD_ICON} width="14" />
		<span class="crumb-where">{underThing ? underThing.title : 'Threads'}</span>
		<Icon icon="mynaui:chevron-right" width="12" />
		<span>New thread</span>
	{/snippet}

	{#snippet lead()}
		<Popover bind:open={glyphOpen} placement="bottom-start" offset={4}>
			{#snippet trigger({ toggle })}
				<button
					type="button"
					class="glyph"
					class:tinted={!!draft.color}
					style={draft.color ? `--tile: ${accentCss(draft.color)}` : undefined}
					aria-label="Choose an icon and color"
					data-tip="Icon and color"
					onclick={toggle}
				>
					<ProjectGlyph project={glyph} size={20} inherit />
				</button>
			{/snippet}
			{#snippet children({ close })}
				<IconPicker
					value={draft.icon}
					onSelect={(v) => (draft.icon = v)}
					{close}
					color={draft.color}
					onColorSelect={(c) => (draft.color = c)}
					label={draft.name || 'New thread'}
					fallback={THREAD_ICON}
				/>
			{/snippet}
		</Popover>
	{/snippet}

	{#snippet chips()}
		<CreateChip icon={draft.backlog ? 'mynaui:archive' : 'mynaui:circle-half'} label={draft.backlog ? 'Backlog' : 'Active'}>
			{#snippet menu({ close })}
				<MenuItem
					role="menuitemradio"
					icon="mynaui:circle-half"
					checked={!draft.backlog}
					label="Active"
					description="It shows in your week and on the Overview."
					onclick={() => {
						draft.backlog = false;
						close();
					}}
				/>
				<MenuItem
					role="menuitemradio"
					icon="mynaui:archive"
					checked={draft.backlog}
					label="Backlog"
					description="It waits until you come back to it."
					onclick={() => {
						draft.backlog = true;
						close();
					}}
				/>
			{/snippet}
		</CreateChip>

		<CreateChip
			icon={draft.automatic ? 'mynaui:sparkles' : 'mynaui:hand'}
			label={draft.automatic ? 'Adding: Automatic' : 'Adding: Manual'}
		>
			{#snippet menu({ close })}
				<MenuItem
					role="menuitemradio"
					icon="mynaui:hand"
					checked={!draft.automatic}
					label="Manual"
					description="Only what you add is filed here."
					onclick={() => {
						draft.automatic = false;
						close();
					}}
				/>
				<MenuItem
					role="menuitemradio"
					icon="mynaui:sparkles"
					checked={draft.automatic}
					label="Automatic"
					description="Your server also files what it is sure belongs here, and asks about the rest."
					onclick={() => {
						draft.automatic = true;
						close();
					}}
				/>
			{/snippet}
		</CreateChip>

		<CreateChip icon="atlas:things" label={underThing ? `Under ${underThing.title}` : 'Not under a thing'} quiet={!underThing}>
			{#snippet menu({ close })}
				<MenuItem
					role="menuitemradio"
					checked={!draft.under}
					label="Not under a thing"
					onclick={() => {
						draft.under = null;
						close();
					}}
				/>
				{#each thingsStore.things as th (th.id)}
					<MenuItem
						role="menuitemradio"
						icon="atlas:things"
						checked={draft.under === th.id}
						label={th.title}
						onclick={() => {
							draft.under = th.id;
							close();
						}}
					/>
				{/each}
			{/snippet}
		</CreateChip>
	{/snippet}
</CreateDialog>

<style>
	.crumb-where {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-foreground);
	}
	/* The thread's glyph in its color at a sixth, as its tile wears it everywhere. */
	.glyph {
		display: grid;
		place-items: center;
		flex: none;
		width: 36px;
		height: 36px;
		padding: 0;
		border: 0;
		border-radius: var(--radius-md);
		background: var(--fill-bg);
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.glyph:hover {
		background: var(--active-bg);
		transition-duration: var(--motion-hover-in);
	}
	.glyph.tinted {
		background: color-mix(in srgb, var(--tile) 16%, transparent);
		color: var(--tile);
	}
	@media (prefers-reduced-motion: reduce) {
		.glyph {
			transition: none;
		}
	}
</style>
