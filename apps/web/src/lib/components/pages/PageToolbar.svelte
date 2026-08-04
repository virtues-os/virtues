<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import IconPicker from "$lib/components/IconPicker.svelte";
	import CoverImagePicker from "$lib/components/CoverImagePicker.svelte";
	import DisplaySettingsPopover from "$lib/components/pages/DisplaySettingsPopover.svelte";
	import { VersionHistoryPanel } from "$lib/components/pages";
	import { Popover } from "$lib/floating";
	import { pageDisplay } from "$lib/stores/pageDisplay.svelte";
	import type { YjsDocument } from "$lib/yjs";

	interface Props {
		icon: string | null;
		/** `--cat-*` token key for the icon, or null. */
		iconColor?: string | null;
		coverUrl: string | null;
		copied: boolean;
		pageId: string;
		yjsDoc: YjsDocument | undefined;
		showCoverPicker?: boolean;
		isShared?: boolean;
		referencesActive?: boolean;
		onIconSelect: (value: string | null) => void;
		onIconColorSelect?: (value: string | null) => void;
		onCoverSelect: (url: string | null) => void;
		onCopyMarkdown: () => void;
		onShare?: () => void;
		onToggleReferences?: () => void;
		onDelete: () => void;
	}

	let {
		icon,
		iconColor = null,
		coverUrl,
		copied,
		pageId,
		yjsDoc,
		showCoverPicker = $bindable(false),
		isShared = false,
		referencesActive = false,
		onIconSelect,
		onIconColorSelect,
		onCoverSelect,
		onCopyMarkdown,
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
<div class="page-toolbar">
	<div class="toolbar-spacer"></div>

	<!-- Identity -->
	<div class="toolbar-group">
		<Popover bind:open={showIconPicker} placement="bottom-start">
			{#snippet trigger({ toggle })}
				<button
					onclick={toggle}
					class="toolbar-action"
					title={icon ? "Change icon" : "Add icon"}
				>
					{#if icon}
						{#if icon.includes(":")}
							<Icon
								{icon}
								width="15"
								style={iconColor ? `color: var(--cat-${iconColor})` : undefined}
							/>
						{:else}
							<span class="toolbar-emoji">{icon}</span>
						{/if}
					{:else}
						<Icon icon="ri:emotion-line" width="15" />
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
				/>
			{/snippet}
		</Popover>
		<Popover bind:open={showCoverPicker} placement="bottom-start">
			{#snippet trigger({ toggle })}
				<button
					onclick={toggle}
					class="toolbar-action"
					title={coverUrl ? "Change cover" : "Add cover"}
				>
					<Icon
						icon={coverUrl ? "ri:image-edit-line" : "ri:image-line"}
						width="15"
					/>
				</button>
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
				>
					Aa
				</button>
			{/snippet}
			{#snippet children()}
				<DisplaySettingsPopover />
			{/snippet}
		</Popover>
		<button
			onclick={() => pageDisplay.toggleSpellcheck()}
			class="toolbar-action"
			class:active={pageDisplay.spellcheck}
			title={pageDisplay.spellcheck ? "Spell check on" : "Spell check off"}
		>
			<Icon icon="ri:check-double-line" width="15" />
		</button>
	</div>

	<div class="toolbar-gap"></div>

	<!-- Actions -->
	<div class="toolbar-group">
		{#if onToggleReferences}
			<button
				onclick={onToggleReferences}
				class="toolbar-action"
				class:active={referencesActive}
				title="References"
			>
				<Icon icon="ri:links-line" width="15" />
			</button>
		{/if}
		<Popover bind:open={showVersionHistory} placement="bottom-end">
			{#snippet trigger({ toggle })}
				<button onclick={toggle} class="toolbar-action" title="Version history">
					<Icon icon="ri:history-line" width="15" />
				</button>
			{/snippet}
			{#snippet children({ close })}
				<VersionHistoryPanel {close} {pageId} {yjsDoc} />
			{/snippet}
		</Popover>
		{#if onShare}
			<button
				onclick={onShare}
				class="toolbar-action"
				class:active={isShared}
				title={isShared ? "Manage share link" : "Share page"}
			>
				<Icon icon={isShared ? "ri:link" : "ri:share-line"} width="15" />
			</button>
		{/if}
		<Popover bind:open={showOverflow} placement="bottom-end">
			{#snippet trigger({ toggle })}
				<button onclick={toggle} class="toolbar-action" title="More">
					<Icon icon="ri:more-2-fill" width="15" />
				</button>
			{/snippet}
			{#snippet children({ close })}
				<div class="overflow-menu">
					<button
						class="overflow-item"
						onclick={() => {
							onCopyMarkdown();
							close();
						}}
					>
						<Icon
							icon={copied ? "ri:check-line" : "ri:file-copy-line"}
							width="15"
						/>
						<span>{copied ? "Copied!" : "Copy as Markdown"}</span>
					</button>
					<div class="overflow-divider"></div>
					{#if showDeleteConfirm}
						<div class="delete-confirm">
							<p class="delete-confirm-text">Delete this page?</p>
							<div class="delete-confirm-actions">
								<button
									class="delete-confirm-btn delete-confirm-cancel"
									onclick={() => (showDeleteConfirm = false)}
								>
									Cancel
								</button>
								<button
									class="delete-confirm-btn delete-confirm-delete"
									onclick={() => {
										onDelete();
										close();
									}}
								>
									Delete
								</button>
							</div>
						</div>
					{:else}
						<button
							class="overflow-item overflow-item-danger"
							onclick={() => (showDeleteConfirm = true)}
						>
							<Icon icon="ri:delete-bin-line" width="15" />
							<span>Delete page</span>
						</button>
					{/if}
				</div>
			{/snippet}
		</Popover>
	</div>
</div>

<style>
	.page-toolbar {
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
		gap: 2px;
	}

	/* Whitespace separates semantic groups instead of hard dividers */
	.toolbar-gap {
		width: 10px;
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
		border-radius: 6px;
		transition:
			color 0.15s ease,
			background-color 0.15s ease;
	}

	.toolbar-action:hover {
		color: var(--color-foreground);
		background: var(--hover-bg);
	}

	.toolbar-action.active {
		color: var(--color-primary);
	}

	.toolbar-action-text {
		font-size: 13px;
		font-weight: 600;
		font-family: var(--font-serif, Georgia, serif);
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

	.overflow-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 7px 10px;
		border: none;
		background: none;
		color: var(--color-foreground);
		font-size: 13px;
		text-align: left;
		border-radius: 6px;
		cursor: pointer;
		transition:
			color 0.12s ease,
			background-color 0.12s ease;
	}

	.overflow-item:hover {
		background: var(--hover-bg);
	}

	.overflow-item-danger {
		color: var(--color-foreground-muted);
	}

	.overflow-item-danger:hover {
		color: var(--color-error);
	}

	.overflow-divider {
		height: 1px;
		background: var(--color-border-subtle, var(--color-border));
		margin: 4px 0;
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

	.delete-confirm-btn {
		padding: 6px 12px;
		font-size: 12px;
		font-weight: 500;
		border: none;
		border-radius: 6px;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.delete-confirm-cancel {
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
	}

	.delete-confirm-cancel:hover {
		background: var(--color-border);
		color: var(--color-foreground);
	}

	.delete-confirm-delete {
		background: var(--color-error);
		color: white;
	}

	.delete-confirm-delete:hover {
		filter: brightness(1.1);
	}
</style>
