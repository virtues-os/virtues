<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import { browser } from "$app/environment";
	import { animate } from "motion";

	interface Props {
		content: string;
	}

	let { content }: Props = $props();

	interface TocItem {
		level: number;
		text: string;
		slug: string;
	}

	let activeSlug: string = $state("");
	let scrollEnabled: boolean = $state(true);
	let circleElement: HTMLDivElement | null = $state(null);
	let headingElements: Map<string, HTMLElement> = new Map();
	let scrollContainer: HTMLElement | null = null;
	let mutationObserver: MutationObserver | null = null;

	// Extract headings from markdown content
	const tocItems = $derived.by(() => {
		const items: TocItem[] = [];
		const lines = content.split("\n");

		for (const line of lines) {
			const match = line.match(/^(#{1,6})\s+(.+)$/);
			if (match) {
				const level = match[1].length;
				const text = match[2].trim();
				const slug = text
					.toLowerCase()
					.replace(/[^a-z0-9]+/g, "-")
					.replace(/^-|-$/g, "");
				items.push({ level, text, slug });
			}
		}

		return items;
	});

	// Find heading elements in the editor by matching text content
	function findHeadingElements() {
		if (!browser) return;

		headingElements.clear();

		// Find the wiki article container (the scrollable parent)
		scrollContainer = document.querySelector(".wiki-article");
		if (!scrollContainer) return;

		// Find all heading lines in CodeMirror editor
		// The wiki-theme applies classes like "cm-heading-line cm-heading-1" to .cm-line elements
		const cmElements = scrollContainer.querySelectorAll(
			".cm-heading-1, .cm-heading-2, .cm-heading-3, .cm-heading-4, .cm-heading-5, .cm-heading-6",
		);

		for (const el of cmElements) {
			// Get the text content (excluding any hidden syntax markers)
			const text = el.textContent?.trim() || "";
			const slug = text
				.toLowerCase()
				.replace(/[^a-z0-9]+/g, "-")
				.replace(/^-|-$/g, "");

			if (slug && !headingElements.has(slug)) {
				headingElements.set(slug, el as HTMLElement);
			}
		}

		// Also find regular HTML headings (h1-h6) for structural sections
		const htmlHeadings = scrollContainer.querySelectorAll(
			"h1, h2, h3, h4, h5, h6",
		);

		for (const el of htmlHeadings) {
			const text = el.textContent?.trim() || "";
			const slug = text
				.toLowerCase()
				.replace(/[^a-z0-9]+/g, "-")
				.replace(/^-|-$/g, "");

			if (slug && !headingElements.has(slug)) {
				headingElements.set(slug, el as HTMLElement);
			}
		}

		// Set initial active heading if not set
		if (tocItems.length > 0 && !activeSlug) {
			activeSlug = tocItems[0].slug;
		}
	}

	// Handle scroll to determine active heading
	function handleScroll() {
		if (!browser || !scrollEnabled || !scrollContainer) return;

		const containerRect = scrollContainer.getBoundingClientRect();
		// Use 25% from the top of the viewport as reference point
		const referencePoint = containerRect.top + containerRect.height * 0.25;

		let closestHeading: string | null = null;
		let closestDistance = Infinity;

		for (const [slug, el] of headingElements) {
			const rect = el.getBoundingClientRect();
			// Find the heading closest to the reference point
			const distance = Math.abs(rect.top - referencePoint);

			if (distance < closestDistance) {
				closestDistance = distance;
				closestHeading = slug;
			}
		}

		// If no heading found, use the first one
		if (!closestHeading && headingElements.size > 0) {
			closestHeading = headingElements.keys().next().value ?? null;
		}

		if (closestHeading && closestHeading !== activeSlug) {
			activeSlug = closestHeading;
		}
	}

	// Animate circle to active TOC item
	function animateCircle() {
		if (!browser || !circleElement || !activeSlug) return;

		const tocElement = document.getElementById(`toc-${activeSlug}`);
		if (!tocElement) return;

		const targetY = tocElement.offsetTop + 10; // Center on the link

		animate(
			circleElement,
			{ y: targetY, opacity: 1, scale: 1 },
			{
				type: "spring",
				stiffness: 500,
				damping: 50,
				duration: 0.3,
			},
		);
	}

	// Handle click on TOC item
	function handleTocClick(slug: string) {
		if (!scrollContainer) return;

		activeSlug = slug;
		scrollEnabled = false;

		// Find the heading element
		const headingEl = headingElements.get(slug);
		if (headingEl) {
			// Calculate the scroll position relative to the scroll container
			const containerRect = scrollContainer.getBoundingClientRect();
			const headingRect = headingEl.getBoundingClientRect();
			const scrollTop = scrollContainer.scrollTop;
			const targetScroll =
				scrollTop + (headingRect.top - containerRect.top) - 20; // 20px offset from top

			scrollContainer.scrollTo({
				top: targetScroll,
				behavior: "smooth",
			});
		}

		// Re-enable scroll tracking after animation completes
		setTimeout(() => {
			scrollEnabled = true;
		}, 800);
	}

	// Set up everything
	function setup(retryCount = 0) {
		if (!browser) return;

		findHeadingElements();

		// If no headings found and we haven't retried too many times, retry
		if (headingElements.size === 0 && retryCount < 5) {
			setTimeout(() => setup(retryCount + 1), 200);
			return;
		}

		// Attach scroll listener to the container
		if (scrollContainer) {
			scrollContainer.addEventListener("scroll", handleScroll, {
				passive: true,
			});
		}

		// Set up MutationObserver to detect when editor content changes
		const editorEl = document.querySelector(".wiki-editor");
		if (editorEl && !mutationObserver) {
			mutationObserver = new MutationObserver(() => {
				// Re-find headings when DOM changes
				findHeadingElements();
			});
			mutationObserver.observe(editorEl, {
				childList: true,
				subtree: true,
				characterData: true,
			});
		}

		// Initial scroll position check
		handleScroll();
	}

	function cleanup() {
		if (scrollContainer) {
			scrollContainer.removeEventListener("scroll", handleScroll);
		}
		if (mutationObserver) {
			mutationObserver.disconnect();
			mutationObserver = null;
		}
	}

	let isMounted = false;
	let previousContent = "";

	onMount(() => {
		if (!browser) return;
		isMounted = true;
		previousContent = content;

		// Delay setup to ensure editor is rendered
		setTimeout(() => {
			setup();
		}, 300);
	});

	onDestroy(() => {
		cleanup();
	});

	// Re-setup when content changes (but not on initial mount)
	$effect(() => {
		if (browser && isMounted && content !== previousContent) {
			previousContent = content;
			// Clean up old listeners
			cleanup();
			// Delay to let editor re-render
			setTimeout(() => {
				setup();
			}, 300);
		}
	});

	// Animate circle when active slug changes
	$effect(() => {
		if (browser && activeSlug) {
			// Small delay to ensure DOM is updated
			setTimeout(() => {
				animateCircle();
			}, 10);
		}
	});
</script>

<div class="rail-contents">
	<div class="contents-header">
		<span class="contents-title">Contents</span>
	</div>

	{#if tocItems.length === 0}
		<div class="contents-empty">
			<p>No headings found</p>
		</div>
	{:else}
		<nav class="contents-nav">
			<!-- Animated circle indicator -->
			<div
				bind:this={circleElement}
				class="contents-circle"
				style="opacity: 0; transform: scale(0);"
			></div>

			<ol class="contents-list">
				{#each tocItems as item}
					<li
						class="contents-item"
						style="--indent: {item.level - 1}"
					>
						<button
							id="toc-{item.slug}"
							class="contents-link"
							class:active={activeSlug === item.slug}
							onclick={() => handleTocClick(item.slug)}
						>
							{item.text}
						</button>
					</li>
				{/each}
			</ol>
		</nav>
	{/if}
</div>

<style>
	.rail-contents {
		padding: 0;
	}

	.contents-header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.75rem 0.5rem 0.75rem 1rem;
		border-bottom: 1px solid var(--color-border-subtle);
		color: var(--color-foreground-muted);
	}

	.contents-title {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.875rem;
		font-weight: 400;
		color: var(--color-foreground);
	}

	.contents-empty {
		padding: 1rem;
	}

	.contents-empty p {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	.contents-nav {
		position: relative;
		padding: 0.5rem 0.5rem 0.5rem 0.5rem;
	}

	.contents-circle {
		position: absolute;
		left: 0.65rem;
		top: 0;
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: var(--color-primary);
		pointer-events: none;
		z-index: 1;
	}

	.contents-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.contents-item {
		padding-left: calc(var(--indent) * 0.75rem);
	}

	.contents-link {
		display: block;
		width: 100%;
		padding: 0.25rem 0.5rem 0.25rem 0.5rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-radius: 0.25rem;
		text-align: left;
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.contents-link:hover {
		color: var(--color-foreground);
	}

	.contents-link.active {
		color: var(--color-foreground);
	}
</style>
