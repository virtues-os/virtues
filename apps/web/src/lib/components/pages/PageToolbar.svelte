<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import MenuItem from "$lib/components/MenuItem.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { projectMenuItems } from "$lib/utils/projectActions";
	import Button from "$lib/components/Button.svelte";
	import IconButton from "$lib/components/IconButton.svelte";
	import DisplaySettingsPopover from "$lib/components/pages/DisplaySettingsPopover.svelte";
	import { VersionHistoryPanel } from "$lib/components/pages";
	import { Popover } from "$lib/floating";
	import type { PageFormat } from "$lib/api/client";

	interface Props {
		copied: boolean;
		pageId: string;
		/** How the page holds its text: only a block page puts a version back. */
		format?: PageFormat;
		isShared?: boolean;
		referencesActive?: boolean;
		onCopyMarkdown: () => void;
		/** The page's counts, said at the foot of ⋯ (block pages). */
		counts?: { words: number; characters: number; links: number } | null;
		/** Open the page's markdown, read only (block pages). */
		onViewMarkdown?: () => void;
		/** Print the page (block pages, which print as A4). */
		onPrint?: () => void;
		onShare?: () => void;
		onToggleReferences?: () => void;
		onDelete: () => void;
		/** Find stands in the toolbar's place (DocumentEditor's dock): the toolbar steps aside. */
		finding?: boolean;
		/** The page's headings, offered as Contents in ⋯ when the pane has no gutter for the headings rail. */
		contents?: { label: string; go: () => void }[] | null;
	}

	let {
		finding = false,
		contents = null,
		copied,
		pageId,
		format = "markdown",
		isShared = false,
		referencesActive = false,
		onCopyMarkdown,
		counts = null,
		onViewMarkdown,
		onPrint,
		onShare,
		onToggleReferences,
		onDelete,
	}: Props = $props();

	let showDisplaySettings = $state(false);
	let showOverflow = $state(false);
	let showVersionHistory = $state(false);
	let showDeleteConfirm = $state(false);

	// Version history opens from ⋯, so focus goes back to ⋯ when it closes,
	// unless something else took it.
	let moreAnchor = $state<HTMLElement | null>(null);
	let historyWasOpen = false;
	$effect(() => {
		const open = showVersionHistory;
		if (historyWasOpen && !open) {
			const active = document.activeElement;
			if (!active || active === document.body) moreAnchor?.querySelector("button")?.focus();
		}
		historyWasOpen = open;
	});
</script>

<!--
  Page toolbar — a light overlay, not a bar. Faded at rest so the document
  reads clean; rises to full opacity on hover/focus. Display and share sit
  on the bar; the long tail lives behind ⋯. The icon and cover are added
  from the page itself, over its title (PageContent), and the save state
  shows in the tab's toolbar only when it is something other than saved.
-->
<div class="page-toolbar" class:finding inert={finding}>
	<div class="toolbar-spacer"></div>

	<!-- Display -->
	<div class="toolbar-group">
		<Popover bind:open={showDisplaySettings} placement="bottom-end">
			{#snippet trigger({ toggle })}
				<button
					onclick={toggle}
					class="toolbar-action toolbar-action-text"
					class:active={showDisplaySettings}
					title="Display settings"
					aria-label="Display settings"
					aria-expanded={showDisplaySettings}
					aria-haspopup="dialog"
				>
					Aa
				</button>
			{/snippet}
			{#snippet children()}
				<DisplaySettingsPopover
					onViewMarkdown={onViewMarkdown
						? () => {
								showDisplaySettings = false;
								onViewMarkdown();
							}
						: undefined}
				/>
			{/snippet}
		</Popover>
	</div>

	<div class="toolbar-gap"></div>

	<!-- Actions -->
	<div class="toolbar-group">
		{#if onShare}
			<!-- No `pressed`. Sharing is not a toggle this button flips: clicking it
			     copies the link and raises a toast, so `aria-pressed="false"` would
			     tell a screen reader "off" about a thing that has no on. The state
			     IS carried — by the icon, which becomes a link once one exists, and
			     by the label, which changes with it. -->
			<IconButton
				icon={isShared ? "mynaui:link" : "mynaui:share"}
				label={isShared ? "Manage share link" : "Share page"}
				onclick={onShare}
			/>
		{/if}
		<!-- Version history opens from ⋯, under it: this empty anchor holds its place. -->
		<Popover bind:open={showVersionHistory} placement="bottom-end">
			{#snippet trigger()}
				<span class="history-anchor" aria-hidden="true"></span>
			{/snippet}
			{#snippet children({ close })}
				<VersionHistoryPanel {close} {pageId} {format} />
			{/snippet}
		</Popover>
		<Popover bind:open={showOverflow} placement="bottom-end">
			{#snippet trigger({ toggle })}
				<span class="more-anchor" bind:this={moreAnchor}>
					<IconButton icon="mynaui:dots-vertical" label="More" expanded={showOverflow} haspopup="menu" onclick={toggle} />
				</span>
			{/snippet}
			{#snippet children({ close })}
				<div class="overflow-menu">
					{#if contents?.length}
						<!-- As Add to project: the headings open as their own menu
						     where this row was. -->
						<MenuItem
							icon="mynaui:list"
							label="Contents…"
							onclick={(e: MouseEvent) => {
								const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
								close();
								const items = (contents ?? []).map((c, i) => ({ id: `heading-${i}`, label: c.label, action: c.go }));
								contextMenu.show({ x: rect.left, y: rect.top }, items, {
									anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
									placement: "bottom-end",
								});
							}}
						/>
						<div class="overflow-divider"></div>
					{/if}
					{#if onToggleReferences}
						<MenuItem
							icon="mynaui:link"
							label="References"
							role="menuitemcheckbox"
							checked={referencesActive}
							onclick={() => {
								close();
								onToggleReferences();
							}}
						/>
					{/if}
					<MenuItem
						icon="mynaui:clock-circle"
						label="Version history"
						onclick={() => {
							close();
							// After this click, so the history's outside-click close doesn't take it.
							requestAnimationFrame(() => (showVersionHistory = true));
						}}
					/>
					<div class="overflow-divider"></div>
					<MenuItem
						icon={copied ? "mynaui:check" : "mynaui:copy"}
						label={copied ? "Copied!" : "Copy as Markdown"}
						onclick={() => {
							onCopyMarkdown();
							close();
						}}
					/>
					{#if onPrint}
						<MenuItem
							icon="mynaui:printer"
							label="Print"
							onclick={() => {
								close();
								onPrint();
							}}
						/>
					{/if}
					<!-- This popover has no submenus, so the choice opens as the
					     shared project menu where this row was. -->
					<MenuItem
						icon="mynaui:folder-plus"
						label="Add to thread…"
						onclick={(e: MouseEvent) => {
							const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
							close();
							const items = projectMenuItems({ url: `/page/${pageId}` })[0]?.submenu ?? [];
							contextMenu.show({ x: rect.left, y: rect.top }, items, {
								anchor: { x: rect.left, y: rect.top, width: rect.width, height: rect.height },
								placement: "bottom-end",
							});
						}}
					/>
					<div class="overflow-divider"></div>
					{#if showDeleteConfirm}
						<div class="delete-confirm">
							<p class="delete-confirm-text">Delete this page?</p>
							<div class="delete-confirm-actions">
								<Button
									variant="secondary"
									size="sm"
									onclick={() => (showDeleteConfirm = false)}
								>
									Cancel
								</Button>
								<Button
									variant="danger"
									size="sm"
									onclick={() => {
										onDelete();
										close();
									}}
								>
									Delete
								</Button>
							</div>
						</div>
					{:else}
						<MenuItem
							icon="mynaui:trash"
							label="Delete page"
							destructive
							onclick={() => (showDeleteConfirm = true)}
						/>
					{/if}
					{#if counts}
						<div class="overflow-divider"></div>
						<p class="overflow-counts">
							{counts.words.toLocaleString()} {counts.words === 1 ? "word" : "words"} ·
							{counts.characters.toLocaleString()} {counts.characters === 1 ? "character" : "characters"} ·
							{counts.links.toLocaleString()} {counts.links === 1 ? "link" : "links"}
						</p>
					{/if}
				</div>
			{/snippet}
		</Popover>
	</div>
</div>

<style>
	/* Out to the left as find comes in from the right, and back as it goes. */
	.page-toolbar.finding {
		opacity: 0;
		transform: translateX(-8px);
	}

	@media (prefers-reduced-motion: reduce) {
		.page-toolbar {
			transition: none;
		}
	}

	.page-toolbar {
		transition:
			opacity 120ms var(--ease-premium),
			transform 120ms var(--ease-premium);
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 6px 12px;
		background: transparent;
		flex-shrink: 0;
		/* Classic always-visible top bar (no fade). */
		opacity: 1;
	}

	.toolbar-spacer {
		flex: 1;
	}

	.toolbar-group {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	/* Whitespace separates semantic groups instead of hard dividers */
	.toolbar-gap {
		width: 8px;
	}

	.toolbar-action {
		display: flex;
		align-items: center;
		justify-content: center;
		min-width: 28px;
		height: 28px;
		padding: 6px;
		border: none;
		background: none;
		color: var(--color-foreground-muted);
		cursor: pointer;
		border-radius: var(--radius-md);
		transition:
			color 0.12s ease,
			background-color 0.12s ease;
	}

	.toolbar-action:hover {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}

	/* Open, as IconButton draws a button whose popover is open: a ground,
	   not the accent, which means a thing is on or can be pressed. */
	.toolbar-action.active {
		color: var(--color-foreground);
		background: var(--active-bg);
	}

	/* The "Aa" that opens display settings. The serif is load-bearing here and
	   stays: the glyphs ARE the control's icon, previewing letterforms the way
	   its siblings preview their verbs. What goes is `font-weight: 600` —
	   the serif ships one cut, so the weight resolved back to the regular and
	   returned silently, and this label had been drawn at 400 all along while
	   the stylesheet claimed otherwise. Sized to 16px so it reads at the
	   optical weight the 600 was asking for and matches the 15px icons beside
	   it; `--font-serif-ui` because it sits in a fixed 28px chrome row, which
	   is exactly the case those corrected vertical metrics exist for. */
	.toolbar-action-text {
		font-size: 16px;
		font-weight: 400;
		font-family: var(--font-serif-ui, Georgia, serif);
	}

	.more-anchor {
		display: contents;
	}

	.history-anchor {
		display: block;
		width: 0;
		height: 28px;
	}

	/* Overflow menu */
	.overflow-menu {
		display: flex;
		flex-direction: column;
		padding: 4px;
		min-width: 190px;
	}

	.overflow-divider {
		height: 1px;
		background: var(--color-border-subtle, var(--color-border));
		margin: 4px 0;
	}

	.overflow-counts {
		margin: 0;
		padding: 4px 10px 6px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.delete-confirm {
		padding: 8px 10px;
	}

	.delete-confirm-text {
		margin: 0 0 10px 0;
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.delete-confirm-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
	}

</style>
