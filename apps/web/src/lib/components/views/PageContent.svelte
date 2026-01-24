<script lang="ts">
	/**
	 * PageContent - Platform-agnostic page editor content
	 * 
	 * Displays and edits a user page. Can be used in tabs, modals, or mobile WebViews.
	 */
	import { Page } from '$lib';
	import { PageEditor } from '$lib/components/pages';
	import { pagesStore } from '$lib/stores/pages.svelte';
	import { onMount, untrack } from 'svelte';
	import 'iconify-icon';

	interface Props {
		/** The page ID to display/edit */
		pageId?: string;
		/** Whether this content is currently active/visible */
		active?: boolean;
		/** Callback when the page title changes (for tab label updates) */
		onLabelChange?: (label: string) => void;
		/** Callback when navigating away (e.g., after delete) */
		onNavigate?: (route: string) => void;
	}

	let { pageId = '', active = true, onLabelChange, onNavigate }: Props = $props();

	interface PageData {
		id: string;
		title: string;
		content: string;
		created_at: string;
		updated_at: string;
	}

	let pageData = $state<PageData | null>(null);
	let title = $state('');
	let content = $state('');
	let loading = $state(false);
	let saving = $state(false);
	let hasSaved = $state(false);
	let saveError = $state(false);
	let error = $state<string | null>(null);
	let saveTimeout: ReturnType<typeof setTimeout> | null = null;

	// Typing state
	let isTyping = $state(false);
	let typingTimeout: ReturnType<typeof setTimeout> | null = null;

	// Computed stats
	const wordCount = $derived(content.trim() ? content.trim().split(/\s+/).length : 0);
	const charCount = $derived(content.length);

	// TODO: Fetch actual backlinks from API
	const backlinks = $state(0);

	// Reset hasSaved after a delay so the checkmark fades
	$effect(() => {
		if (hasSaved) {
			const timeout = setTimeout(() => {
				hasSaved = false;
			}, 3000);
			return () => clearTimeout(timeout);
		}
	});

	// Track the last loaded pageId to avoid reloading the same page
	let lastLoadedPageId = $state<string | null>(null);

	onMount(async () => {
		console.log('[PageContent] onMount, pageId:', pageId, 'active:', active);
		if (pageId && pageId !== lastLoadedPageId) {
			lastLoadedPageId = pageId;
			await loadPage();
		}
	});

	// Reload only when pageId actually changes to a new value
	// Use untrack() to prevent infinite loops from state updates
	$effect(() => {
		const currentPageId = pageId;
		const isActive = active;
		
		// Only reload if pageId changed to a different value
		if (currentPageId && isActive) {
			untrack(() => {
				if (currentPageId !== lastLoadedPageId) {
					console.log('[PageContent] pageId changed, reloading:', currentPageId);
					lastLoadedPageId = currentPageId;
					loadPage();
				}
			});
		}
	});

	async function loadPage() {
		if (!pageId) {
			error = 'No page ID provided';
			loading = false;
			return;
		}

		loading = true;
		error = null;

		try {
			const response = await fetch(`/api/pages/${pageId}`);
			if (!response.ok) {
				if (response.status === 404) {
					error = 'Page not found';
				} else {
					throw new Error('Failed to load page');
				}
				return;
			}
			const data: PageData = await response.json();
			pageData = data;
			title = data.title;
			content = data.content;

			// Update label
			onLabelChange?.(data.title);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load page';
		} finally {
			loading = false;
		}
	}

	function scheduleSave() {
		if (saveTimeout) clearTimeout(saveTimeout);
		saveTimeout = setTimeout(() => {
			save();
		}, 1000);
	}

	function handleContentChange(newContent: string) {
		content = newContent;
		
		// Set typing state
		isTyping = true;
		hasSaved = false;
		saveError = false;
		if (typingTimeout) clearTimeout(typingTimeout);
		typingTimeout = setTimeout(() => {
			isTyping = false;
		}, 1000);

		scheduleSave();
	}

	// Watch for title changes and sync to stores immediately
	$effect(() => {
		if (pageData && title !== undefined) {
			const currentTitle = title.trim() || 'Untitled';
			
			// Update the tab label at the top
			untrack(() => {
				onLabelChange?.(currentTitle);
				
				// Update the sidebar tree item locally for instant feedback
				pagesStore.updatePageLocally(pageData.id, { title: currentTitle });
			});
			
			// Schedule the actual database save
			if (title !== pageData.title) {
				// Set typing state for title changes too
				isTyping = true;
				hasSaved = false;
				saveError = false;
				if (typingTimeout) clearTimeout(typingTimeout);
				typingTimeout = setTimeout(() => {
					isTyping = false;
				}, 1000);

				scheduleSave();
			}
		}
	});

	async function save() {
		if (!pageData || saving) return;
		
		// Clear typing state when saving starts
		isTyping = false;
		if (typingTimeout) clearTimeout(typingTimeout);
		
		saving = true;
		saveError = false;
		hasSaved = false;

		try {
			const response = await fetch(`/api/pages/${pageData.id}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ title, content }),
			});

			if (!response.ok) {
				throw new Error('Failed to save page');
			}

			hasSaved = true;

			// Update internal state to match saved title
			if (pageData) {
				pageData.title = title;
			}
		} catch (err) {
			console.error('Failed to save page:', err);
			saveError = true;
		} finally {
			saving = false;
		}
	}

	async function deletePage() {
		if (!pageData) return;
		if (!confirm('Are you sure you want to delete this page?')) return;

		try {
			const response = await fetch(`/api/pages/${pageData.id}`, {
				method: 'DELETE',
			});

			if (!response.ok) {
				throw new Error('Failed to delete page');
			}

			// Sync with sidebar pages store
			pagesStore.removePage(pageData.id);

			// Navigate to pages list
			onNavigate?.('/pages');
		} catch (err) {
			console.error('Failed to delete page:', err);
		}
	}

	function handleBackClick() {
		onNavigate?.('/pages');
	}
</script>

<Page>
	{#if loading}
		<div class="text-center py-12 text-foreground-muted">Loading...</div>
	{:else if error}
		<div class="max-w-2xl">
			<div class="p-4 bg-error-subtle border border-error rounded-lg text-error">
				{error}
			</div>
			<button onclick={handleBackClick} class="text-primary hover:underline mt-4 inline-block">
				Back to Pages
			</button>
		</div>
	{:else if pageData}
		<div class="max-w-3xl">
			<!-- Status Bar -->
			<div class="page-status-bar">
				<div class="status-item" title="Pages linking to this page">
					<iconify-icon icon="ri:links-line" width="14"></iconify-icon>
					<span>{backlinks} backlinks</span>
				</div>
				<div class="status-divider"></div>
				<div class="status-item" title="Word count">
					<span>{wordCount.toLocaleString()} words</span>
				</div>
				<div class="status-divider"></div>
				<div class="status-item" title="Character count">
					<span>{charCount.toLocaleString()} chars</span>
				</div>
				<div class="status-spacer"></div>
				<div
					class="status-item status-save"
					class:saving
					class:saved={hasSaved}
					class:error={saveError}
					class:typing={isTyping}
				>
					{#if saving}
						<iconify-icon icon="ri:loader-4-line" width="14" class="spin"></iconify-icon>
						<span>Saving</span>
					{:else if saveError}
						<iconify-icon icon="ri:close-line" width="14"></iconify-icon>
						<span>Error</span>
					{:else if hasSaved}
						<iconify-icon icon="ri:check-line" width="14"></iconify-icon>
						<span>Saved</span>
					{:else if isTyping}
						<div class="typing-icon">
							<svg width="14" height="14" viewBox="0 0 16 16" fill="none">
								<text
									x="1"
									y="12"
									font-family="var(--font-serif)"
									font-size="11"
									font-weight="500"
									fill="currentColor">Aa</text
								>
							</svg>
						</div>
						<span>Typing</span>
					{:else}
						<div class="status-dot"></div>
						<span>Ready</span>
					{/if}
				</div>
			</div>

			<!-- Header -->
			<div class="mb-6 flex items-start justify-between">
				<div class="flex-1">
					<input
						type="text"
						bind:value={title}
						placeholder="Untitled"
						class="w-full text-3xl font-serif font-medium text-foreground bg-transparent border-none outline-none placeholder:text-foreground-subtle"
					/>
				</div>
				<button
					onclick={deletePage}
					class="p-2 text-foreground-muted hover:text-error transition-colors"
					title="Delete page"
				>
					<iconify-icon icon="ri:delete-bin-line" width="18"></iconify-icon>
				</button>
			</div>

			<!-- Editor -->
			<div class="min-h-[400px]">
				<PageEditor
					bind:content
					onSave={handleContentChange}
					placeholder="Start writing... Use @ to link pages, people, places, and files."
				/>
			</div>
		</div>
	{/if}
</Page>

<style>
	/* Page Status Bar */
	.page-status-bar {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 0;
		margin-bottom: 16px;
		border-bottom: 1px solid var(--color-border-subtle);
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.status-item {
		display: flex;
		align-items: center;
		gap: 5px;
	}

	.status-divider {
		width: 1px;
		height: 12px;
		background: var(--color-border);
	}

	.status-spacer {
		flex: 1;
	}

	/* Save status states */
	.status-save {
		transition: color 0.2s ease;
	}

	.status-save.saving {
		color: var(--color-foreground-muted);
	}

	.status-save.saved {
		color: var(--color-success);
	}

	.status-save.error {
		color: var(--color-error);
	}

	.status-save.typing {
		color: var(--color-primary);
	}

	.typing-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		animation: pulse 1.5s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 0.6; transform: scale(0.95); }
		50% { opacity: 1; transform: scale(1.05); }
	}

	/* Idle dot indicator */
	.status-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--color-foreground-subtle);
		opacity: 0.6;
	}

	/* Spinning animation */
	.spin {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
