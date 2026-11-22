<!--
	QuickAdd.svelte

	The + on the phone's empty chat. It opens a glass menu above the button
	with three kinds of thing, kept apart by shape: places to go are rows in
	a list, actions are pills. Go to holds Inbox and your pins (else your
	latest threads). Add to this chat attaches a photo or a file to the chat
	you are starting. New makes a task, a thread or a page. A search field
	sits over the chat bar while the menu is open, since your thumb is
	already there.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import AtlasIcon from "$lib/components/sidebar/AtlasIcon.svelte";
	import ProjectGlyph from "$lib/components/ProjectGlyph.svelte";
	import { ATTACH_ACCEPT } from "$lib/components/chat/state/attachments.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { projectStore } from "$lib/stores/project.svelte";
	import { pinsStore } from "$lib/stores/pins.svelte";
	import { pinIdentity } from "$lib/refs/identity.svelte";
	import { search } from "$lib/stores/search.svelte";
	import { newPage, newTask } from "$lib/utils/newActions";
	import { newProject } from "$lib/utils/projectActions";
	import { kindGlyph } from "$lib/utils/iconHelpers";
	import { openWebLink } from "$lib/utils/webLinks";

	let { onFiles }: { onFiles: (files: File[]) => void } = $props();

	let open = $state(false);
	let cameraEl: HTMLInputElement | null = $state(null);
	let libraryEl: HTMLInputElement | null = $state(null);
	let fileEl: HTMLInputElement | null = $state(null);

	const GO_CAP = 3;

	type Place = { key: string; title: string; note: string; run: () => void; thread?: (typeof projectStore.projects)[number]; glyph?: string };

	function go(route: string, label: string) {
		open = false;
		windowShellStore.openTabFromRoute(route, { label });
	}

	// Inbox, then what you pinned, then your threads, until three.
	const places = $derived.by((): Place[] => {
		const out: Place[] = [{ key: "inbox", title: "Inbox", note: "", glyph: "inbox", run: () => go("/inbox", "Inbox") }];
		for (const pin of pinsStore.pins) {
			if (out.length >= GO_CAP) break;
			const look = pinIdentity(pin);
			out.push({
				key: `pin:${pin.id}`,
				title: look.title,
				note: "Pinned",
				glyph: kindGlyph(pin.url),
				run: () => {
					open = false;
					if (/^https?:\/\//i.test(pin.url)) void openWebLink(pin.url);
					else windowShellStore.openTabFromRoute(pin.url, { label: look.title });
				},
			});
		}
		for (const t of projectStore.projects) {
			if (out.length >= GO_CAP) break;
			out.push({ key: `thread:${t.id}`, title: t.name, note: "Thread", thread: t, run: () => go(`/thread/${t.id}`, t.name) });
		}
		return out;
	});

	function picked(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		if (input.files && input.files.length > 0) onFiles(Array.from(input.files));
		input.value = "";
		open = false;
	}

	function pick(el: HTMLInputElement | null) {
		el?.click();
	}

	function act(run: () => void) {
		open = false;
		run();
	}

	function onKey(e: KeyboardEvent) {
		if (open && e.key === "Escape") open = false;
	}
</script>

<svelte:window onkeydown={onKey} />

<input bind:this={cameraEl} type="file" accept="image/*" capture="environment" class="sr-only" onchange={picked} />
<input bind:this={libraryEl} type="file" accept="image/*" multiple class="sr-only" onchange={picked} />
<input bind:this={fileEl} type="file" accept={ATTACH_ACCEPT} multiple class="sr-only" onchange={picked} />

{#if open}
	<!-- A tap anywhere else closes it. Nothing is blurred: the menu's own
	     glass is the only blur. -->
	<button class="scrim" aria-label="Close" onclick={() => (open = false)}></button>

	<div class="menu lq" role="menu" aria-label="Quick add">
		<div class="label">Go to</div>
		{#each places as p (p.key)}
			<button class="place" role="menuitem" onclick={p.run}>
				<span class="glyph">
					{#if p.thread}
						<ProjectGlyph project={p.thread} size={16} />
					{:else}
						<AtlasIcon name={p.glyph ?? "inbox"} size={16} bare />
					{/if}
				</span>
				<span class="place-title">{p.title}</span>
				{#if p.note}<span class="note">{p.note}</span>{/if}
			</button>
		{/each}

		<div class="rule"></div>
		<div class="label">Add to this chat</div>
		<div class="pills">
			<button class="pill" role="menuitem" onclick={() => pick(cameraEl)}>
				<Icon icon="mynaui:camera" width={17} />Camera
			</button>
			<button class="pill" role="menuitem" onclick={() => pick(libraryEl)}>
				<Icon icon="mynaui:image" width={17} />Library
			</button>
			<button class="pill" role="menuitem" onclick={() => pick(fileEl)}>
				<Icon icon="mynaui:paperclip" width={17} />File
			</button>
		</div>

		<div class="label">New</div>
		<div class="pills">
			<button class="pill" role="menuitem" onclick={() => act(newTask)}>
				<AtlasIcon name="tasks" size={17} bare />Task
			</button>
			<button class="pill" role="menuitem" onclick={() => act(() => newProject(go))}>
				<AtlasIcon name="projects" size={17} bare />Thread
			</button>
			<button class="pill" role="menuitem" onclick={() => act(() => void newPage(go))}>
				<AtlasIcon name="pages" size={17} bare />Page
			</button>
		</div>
	</div>

	<button class="search" onclick={() => act(() => search.show())}>
		<AtlasIcon name="search" size={17} bare />
		<span>Search or go to</span>
	</button>
{/if}

<button
	class="fab"
	class:open
	onclick={() => (open = !open)}
	aria-expanded={open}
	aria-haspopup="menu"
	aria-label={open ? "Close" : "Add a task, thread, page or attachment"}
>
	<Icon icon="mynaui:plus" width={20} />
</button>

<style>
	.scrim {
		position: fixed;
		inset: 0;
		z-index: 30;
		border: 0;
		background: transparent;
		cursor: default;
	}

	.fab,
	.menu,
	.search {
		z-index: 31;
	}

	.fab {
		position: absolute;
		right: 16px;
		bottom: calc(16px + max(env(safe-area-inset-bottom) - var(--keyboard-inset, 0px), 0px));
		display: flex;
		align-items: center;
		justify-content: center;
		width: 52px;
		height: 52px;
		border: 0;
		border-radius: var(--radius-full);
		background: var(--color-foreground);
		color: var(--color-surface);
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
		transition: transform 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
	}
	.fab.open {
		transform: rotate(45deg);
	}
	.fab:active {
		transform: scale(0.92);
		transition-duration: 0.08s;
	}
	.fab.open:active {
		transform: rotate(45deg) scale(0.92);
	}

	.menu {
		position: absolute;
		left: 16px;
		right: 16px;
		bottom: calc(80px + max(env(safe-area-inset-bottom) - var(--keyboard-inset, 0px), 0px));
		display: flex;
		flex-direction: column;
		padding: 12px;
		border-radius: var(--radius-xl);
		transform-origin: 100% 100%;
		animation: grow 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
	}
	@keyframes grow {
		from {
			opacity: 0;
			transform: scale(0.9) translateY(8px);
		}
	}

	.label {
		padding: 4px 4px 8px;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
	.pills + .label {
		padding-top: 12px;
	}

	.rule {
		height: 1px;
		margin: 8px 4px;
		background: var(--color-border);
	}

	.place {
		display: flex;
		align-items: center;
		gap: 12px;
		min-height: 44px;
		padding: 0 8px;
		border: 0;
		border-radius: var(--radius-xl);
		background: transparent;
		color: var(--color-foreground);
		font: inherit;
		font-size: 1rem;
		text-align: left;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}
	.glyph {
		flex: none;
		display: flex;
		width: 16px;
		justify-content: center;
	}
	.place-title {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.note {
		flex: none;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	.pills {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 8px;
	}
	.pill {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		height: 44px;
		border: 0;
		border-radius: var(--radius-full);
		background: color-mix(in srgb, var(--color-surface) 60%, transparent);
		color: var(--color-foreground);
		font: inherit;
		font-size: 0.9375rem;
		white-space: nowrap;
		cursor: pointer;
		-webkit-tap-highlight-color: transparent;
	}

	.search {
		position: absolute;
		left: 16px;
		right: 76px;
		bottom: calc(16px + max(env(safe-area-inset-bottom) - var(--keyboard-inset, 0px), 0px));
		display: flex;
		align-items: center;
		gap: 8px;
		height: 52px;
		padding: 0 16px;
		border: 1px solid var(--color-border-strong);
		border-radius: var(--radius-full);
		background: var(--color-surface);
		color: var(--color-foreground-muted);
		font: inherit;
		font-size: 1rem;
		cursor: pointer;
	}

	.place,
	.pill,
	.search {
		transition:
			transform 0.32s cubic-bezier(0.34, 1.56, 0.64, 1),
			background-color 0.2s ease-out;
	}
	.place:active,
	.pill:active,
	.search:active {
		transform: scale(0.97);
		background-color: color-mix(in srgb, var(--wash-ink) 7%, transparent);
		transition-duration: 0.08s;
	}

	@media (prefers-reduced-motion: reduce) {
		.menu {
			animation: none;
		}
		.fab,
		.place,
		.pill,
		.search {
			transition: none;
		}
		.fab:active,
		.place:active,
		.pill:active,
		.search:active {
			transform: none;
		}
		.fab.open,
		.fab.open:active {
			transform: rotate(45deg);
		}
	}
</style>
