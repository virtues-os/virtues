<!--
	DrawerAdd.svelte

	The + beside New chat at the foot of the phone's drawer. Like quick add,
	a glass menu grows from the button and the + turns into a chevron. The
	screen behind dims and softens, so the menu stands out over the list.
	The drawer is a stacking context of its own (the shell moves it), so the
	open menu, its scrim and the chevron that closes it all render on the
	body, the chevron exactly over the +.
	It makes something new (a task, a thread, a page) or puts something into
	the Inbox for later (a note, a photo, a file), which becomes an
	"Inboxed: …" task due today (utils/inboxCapture.ts).
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import AtlasIcon from "$lib/components/sidebar/AtlasIcon.svelte";
	import BottomSheet from "$lib/components/mobile/BottomSheet.svelte";
	import { ATTACH_ACCEPT } from "$lib/components/chat/state/attachments.svelte";
	import { portal } from "$lib/actions/portal";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { newPage, newTask } from "$lib/utils/newActions";
	import { newProject } from "$lib/utils/projectActions";
	import { captureFile, captureNote } from "$lib/utils/inboxCapture";
	import { tapHaptic } from "$lib/utils/haptics";
	import { toast } from "svelte-sonner";

	let open = $state(false);
	let noteOpen = $state(false);
	let noteText = $state("");
	let saving = $state(false);
	let photoEl: HTMLInputElement | null = $state(null);
	let fileEl: HTMLInputElement | null = $state(null);

	function go(route: string, label: string) {
		mobileLayout.closeDrawer();
		windowShellStore.openTabFromRoute(route, { label });
	}

	function toggle() {
		open = !open;
		if (open) tapHaptic();
	}

	function act(run: () => void) {
		tapHaptic();
		open = false;
		run();
	}

	async function inboxFiles(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const files = Array.from(input.files ?? []);
		input.value = "";
		open = false;
		if (!files.length) return;
		try {
			for (const f of files) await captureFile(f);
			toast.success(files.length === 1 ? "Added to your Inbox" : `Added ${files.length} files to your Inbox`);
		} catch (err) {
			console.error("[DrawerAdd] capture failed:", err);
			toast.error("That didn't reach your Inbox. Check your connection and try again.");
		}
	}

	async function saveNote() {
		if (!noteText.trim() || saving) return;
		saving = true;
		try {
			await captureNote(noteText);
			noteText = "";
			noteOpen = false;
			tapHaptic();
			toast.success("Added to your Inbox");
		} catch (err) {
			console.error("[DrawerAdd] note failed:", err);
			toast.error("That note didn't reach your Inbox. Check your connection and try again.");
		} finally {
			saving = false;
		}
	}

	function onKey(e: KeyboardEvent) {
		if (open && e.key === "Escape") open = false;
	}
</script>

<svelte:window onkeydown={onKey} />

<input bind:this={photoEl} type="file" accept="image/*" multiple class="sr-only" onchange={inboxFiles} />
<input bind:this={fileEl} type="file" accept={ATTACH_ACCEPT} multiple class="sr-only" onchange={inboxFiles} />

{#if open}
	<div class="layer" use:portal>
		<button class="scrim" aria-label="Close" onclick={() => (open = false)}></button>
		<div class="menu lq" role="menu" aria-label="New">
			<div class="label">New</div>
			<button class="row" role="menuitem" onclick={() => act(newTask)}>
				<AtlasIcon name="tasks" size={18} bare />
				<span class="text"><span class="name">Task</span><span class="hint">Something to do, with a date if you want</span></span>
			</button>
			<button class="row" role="menuitem" onclick={() => act(() => newProject(go))}>
				<AtlasIcon name="projects" size={18} bare />
				<span class="text"><span class="name">Thread</span><span class="hint">A line of work for chats and pages</span></span>
			</button>
			<button class="row" role="menuitem" onclick={() => act(() => void newPage(go))}>
				<AtlasIcon name="pages" size={18} bare />
				<span class="text"><span class="name">Page</span><span class="hint">A blank page to write in</span></span>
			</button>
			<div class="rule"></div>
			<div class="label">Into your Inbox</div>
			<button class="row" role="menuitem" onclick={() => act(() => (noteOpen = true))}>
				<Icon icon="mynaui:edit" width={18} />
				<span class="text"><span class="name">Note</span><span class="hint">Type it now, file it later</span></span>
			</button>
			<button class="row" role="menuitem" onclick={() => photoEl?.click()}>
				<Icon icon="mynaui:image" width={18} />
				<span class="text"><span class="name">Photo</span><span class="hint">Take one or pick from your library</span></span>
			</button>
			<button class="row" role="menuitem" onclick={() => fileEl?.click()}>
				<Icon icon="mynaui:paperclip" width={18} />
				<span class="text"><span class="name">File</span><span class="hint">A document or PDF</span></span>
			</button>
		</div>
		<button class="plus open" onclick={toggle} aria-expanded="true" aria-label="Close">
			<Icon icon="mynaui:chevron-down" width={20} />
		</button>
	</div>
{/if}

<button class="plus" onclick={toggle} aria-expanded={open} aria-haspopup="menu" aria-label="New, or add to your Inbox">
	<Icon icon="mynaui:plus" width={20} />
</button>

{#if noteOpen}
	<BottomSheet label="Note to your Inbox" onClose={() => (noteOpen = false)}>
		<div class="note">
			<label class="note-label" for="inbox-note">Note to your Inbox</label>
			<!-- svelte-ignore a11y_autofocus -->
			<textarea id="inbox-note" bind:value={noteText} rows="4" placeholder="Type or dictate with the keyboard's mic" autofocus></textarea>
			<button class="save" onclick={saveNote} disabled={!noteText.trim() || saving}>Add to Inbox</button>
		</div>
	</BottomSheet>
{/if}

<style>
	.layer {
		display: contents;
	}

	/* The desktop modal's dim, with a little softening, so the menu stands
	   out over the list. */
	.scrim {
		position: fixed;
		inset: 0;
		z-index: var(--z-context-menu);
		border: 0;
		background: color-mix(in srgb, var(--wash-ink) 18%, transparent);
		-webkit-backdrop-filter: blur(2px);
		backdrop-filter: blur(2px);
		cursor: default;
		animation: fade 0.2s ease-out;
	}

	.menu {
		position: fixed;
		right: 16px;
		bottom: calc(76px + env(safe-area-inset-bottom));
		z-index: var(--z-context-menu);
		display: flex;
		flex-direction: column;
		width: min(264px, calc(100vw - 32px));
		padding: 8px;
		border-radius: var(--radius-xl);
		transform-origin: 100% 100%;
		animation: grow 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
	}
	@keyframes grow {
		from {
			opacity: 0;
			transform: scale(0.92) translateY(8px);
		}
	}
	@keyframes fade {
		from {
			opacity: 0;
		}
	}

	.label {
		padding: 4px 8px;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
	.rule {
		height: 1px;
		margin: 4px 8px;
		background: var(--color-border);
	}

	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 52px;
		padding: 4px 8px;
		border: 0;
		border-radius: var(--radius-lg);
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition:
			transform 0.32s cubic-bezier(0.34, 1.56, 0.64, 1),
			background-color 0.2s ease-out;
	}
	.row:active {
		transform: scale(0.98);
		background: color-mix(in srgb, var(--wash-ink) 7%, transparent);
		transition-duration: 0.08s;
	}
	.text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.name {
		font-size: 1rem;
	}
	.hint {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	/* The same size and corners as New chat beside it. */
	.plus {
		flex: none;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 48px;
		height: 48px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		color: var(--color-foreground);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: transform 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
	}
	.plus.open {
		position: fixed;
		right: 16px;
		bottom: calc(16px + env(safe-area-inset-bottom));
		z-index: var(--z-context-menu);
		border-color: transparent;
		background: var(--color-foreground);
		color: var(--color-surface);
	}
	.plus:active {
		transform: scale(0.92);
		transition-duration: 0.08s;
	}

	.note {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 4px 4px 8px;
	}
	.note-label {
		font-family: var(--font-serif);
		font-size: 1.375rem;
	}
	.note textarea {
		min-height: 120px;
		padding: 12px;
		border: 1px solid var(--color-border-strong);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		color: var(--color-foreground);
		font: inherit;
		font-size: 1rem;
		resize: none;
	}
	.save {
		height: 48px;
		border: 0;
		border-radius: var(--radius-xl);
		background: var(--color-foreground);
		color: var(--color-surface);
		font: inherit;
		font-size: 1rem;
		cursor: pointer;
	}
	.save:disabled {
		opacity: 0.4;
	}

	@media (prefers-reduced-motion: reduce) {
		.menu,
		.scrim {
			animation: none;
		}
		.row,
		.plus {
			transition: none;
		}
		.row:active,
		.plus:active {
			transform: none;
		}
	}
</style>
