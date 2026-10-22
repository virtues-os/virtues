<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import MenuItem from "$lib/components/MenuItem.svelte";
	import { contextMenu } from "$lib/stores/contextMenu.svelte";
	import { projectMenuItems } from "$lib/utils/projectActions";
	import Button from "$lib/components/Button.svelte";
	import IconButton from "$lib/components/IconButton.svelte";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import CoverImagePicker from "$lib/components/CoverImagePicker.svelte";
	import DisplaySettingsPopover from "$lib/components/pages/DisplaySettingsPopover.svelte";
	import { VersionHistoryPanel } from "$lib/components/pages";
	import { Popover } from "$lib/floating";
	import type { PageFormat } from "$lib/api/client";
	import { identityOf } from "$lib/refs/identity.svelte";
	import { clothFor } from "$lib/sidebar/pin-colors";

	interface Props {
		icon: string | null;
		/** `--cat-*` token key for the icon, or null. */
		iconColor?: string | null;
		coverUrl: string | null;
		copied: boolean;
		pageId: string;
		/** How the page holds its text: only a block page puts a version back. */
		format?: PageFormat;
		showCoverPicker?: boolean;
		isShared?: boolean;
		referencesActive?: boolean;
		onIconSelect: (value: string | null) => void;
		onIconColorSelect?: (value: string | null) => void;
		onCoverSelect: (url: string | null) => void;
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
		icon,
		iconColor = null,
		coverUrl,
		copied,
		pageId,
		format = "markdown",
		showCoverPicker = $bindable(false),
		isShared = false,
		referencesActive = false,
		onIconSelect,
		onIconColorSelect,
		onCoverSelect,
		onCopyMarkdown,
		counts = null,
		onViewMarkdown,
		onPrint,
		onShare,
		onToggleReferences,
		onDelete,
	}: Props = $props();

	let showIconPicker = $state(false);
	let showDisplaySettings = $state(false);
	let showOverflow = $state(false);
	let showVersionHistory = $state(false);
	let showDeleteConfirm = $state(false);
</script>

<!--
  Page toolbar — a light overlay, not a bar. Faded at rest so the document
  reads clean; rises to full opacity on hover/focus. Page-level actions only
  (icon, cover, display, share); the long tail lives behind ••• overflow.
-->
<div class="page-toolbar" class:finding inert={finding}>
	<div class="toolbar-spacer"></div>

	<!-- Identity -->
	<div class="toolbar-group">
		<Popover bind:open={showIconPicker} placement="bottom-start">
			{#snippet trigger({ toggle })}
				<button
					onclick={toggle}
					class="toolbar-action"
					title={icon ? "Change icon" : "Add icon"}
					aria-label={icon ? "Change icon" : "Add icon"}
					aria-expanded={showIconPicker}
					aria-haspopup="dialog"
				>
					{#if icon}
						{#if icon.includes(":")}
							<Icon
								{icon}
								width="16"
								style={iconColor ? `color: var(--cat-${iconColor})` : undefined}
							/>
						{:else}
							<span class="toolbar-emoji">{icon}</span>
						{/if}
					{:else}
						<Icon icon="ri:emotion-line" width="16" />
					{/if}
				</button>
			{/snippet}
			{#snippet children({ close })}
				<IconPicker
					value={icon}
					onSelect={onIconSelect}
					{close}
					color={iconColor}
					onColorSelect={onIconColorSelect}
					label={identityOf(`/page/${pageId}`).title}
					autoColor={clothFor({ url: `/page/${pageId}` })}
				/>
			{/snippet}
		</Popover>
		<Popover bind:open={showCoverPicker} placement="bottom-start">
			{#snippet trigger({ toggle })}
				<IconButton
					icon={coverUrl ? "ri:image-edit-line" : "ri:image-line"}
					label={coverUrl ? "Change cover" : "Add cover"}
					expanded={showCoverPicker}
					haspopup="dialog"
					onclick={toggle}
				/>
			{/snippet}
			{#snippet children({ close })}
				<CoverImagePicker value={coverUrl} onSelect={onCoverSelect} {close} />
			{/snippet}
		</Popover>
	</div>

	<div class="toolbar-gap"></div>

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
		{#if onToggleReferences}
			<IconButton
				icon="ri:links-line"
				label="References"
				pressed={referencesActive}
				onclick={onToggleReferences}
			/>
		{/if}
		<Popover bind:open={showVersionHistory} placement="bottom-end">
			{#snippet trigger({ toggle })}
				<IconButton icon="ri:history-line" label="Version history" expanded={showVersionHistory} haspopup="dialog" onclick={toggle} />
			{/snippet}
			{#snippet children({ close })}
				<VersionHistoryPanel {close} {pageId} {format} />
			{/snippet}
		</Popover>
		{#if onShare}
			<!-- No `pressed`. Sharing is not a toggle this button flips: clicking it
			     copies the link and raises a toast, so `aria-pressed="false"` would
			     tell a screen reader "off" about a thing that has no on. The state
			     IS carried — by the icon, which becomes a link once one exists, and
			     by the label, which changes with it. -->
			<IconButton
				icon={isShared ? "ri:link" : "ri:share-line"}
				label={isShared ? "Manage share link" : "Share page"}
				onclick={onShare}
			/>
		{/if}
		<Popover bind:open={showOverflow} placement="bottom-end">
			{#snippet trigger({ toggle })}
				<IconButton icon="ri:more-2-fill" label="More" expanded={showOverflow} haspopup="menu" onclick={toggle} />
			{/snippet}
			{#snippet children({ close })}
				<div class="overflow-menu">
					{#if contents?.length}
						<!-- As Add to project: the headings open as their own menu
						     where this row was. -->
						<MenuItem
							icon="ri:list-unordered"
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
					<MenuItem
						icon={copied ? "ri:check-line" : "ri:file-copy-line"}
						label={copied ? "Copied!" : "Copy as Markdown"}
						onclick={() => {
							onCopyMarkdown();
							close();
						}}
					/>
					{#if onPrint}
						<MenuItem
							icon="ri:printer-line"
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
						icon="ri:folder-add-line"
						label="Add to project…"
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
							icon="ri:delete-bin-line"
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

	.toolbar-action:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
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

	.toolbar-emoji {
		font-size: 14px;
		line-height: 1;
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
