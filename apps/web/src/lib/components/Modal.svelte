<script lang="ts">
	import { portal } from "$lib/actions/portal";
	import { fade, floatOut } from "$lib/utils/transitions";
	import Icon from "$lib/components/Icon.svelte";
	import type { Snippet } from "svelte";

	interface Props {
		open?: boolean;
		onClose: () => void;
		title?: string;
		width?: "sm" | "md" | "lg" | "xl";
		/** The contents draw their own frame: no header, footer or padding. */
		bare?: boolean;
		/** The dialog's name for a screen reader, when there is no `title`. */
		label?: string;
		children: Snippet;
		footer?: Snippet;
	}

	let {
		open = false,
		onClose,
		title,
		width = "md",
		bare = false,
		label,
		children,
		footer,
	}: Props = $props();

	/** Focus moves into the dialog as it opens, and back to whatever had it
	 *  when it closes: otherwise it stays on the page behind the backdrop,
	 *  and Tab walks controls nobody can see. The dialog itself takes it, so
	 *  its label is read first and Tab goes on into its contents. */
	function holdFocus(node: HTMLElement) {
		const before = document.activeElement as HTMLElement | null;
		// After the contents mount: a field that focuses itself keeps it.
		requestAnimationFrame(() => {
			if (!node.contains(document.activeElement)) node.focus({ preventScroll: true });
		});
		return {
			destroy() {
				if (before?.isConnected) before.focus({ preventScroll: true });
			},
		};
	}

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	// Handle escape key globally when open
	$effect(() => {
		if (open) {
			const handler = (e: KeyboardEvent) => {
				// A picker open inside the dialog takes the Escape first and
				// marks it handled; only the next one closes the dialog.
				if (e.key === "Escape" && !e.defaultPrevented) {
					e.preventDefault();
					onClose();
				}
			};
			window.addEventListener("keydown", handler);
			return () => window.removeEventListener("keydown", handler);
		}
	});

	const widthClass = $derived({
		sm: "max-w-sm",
		md: "max-w-md",
		lg: "max-w-lg",
		xl: "max-w-xl",
	}[width]);
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
	<div class="modal-backdrop" onclick={handleBackdropClick} role="presentation" use:portal out:fade>
		<div class="modal {widthClass}" class:bare role="dialog" aria-modal="true" aria-label={title ?? label} tabindex="-1" use:holdFocus out:floatOut>
			{#if title && !bare}
				<div class="modal-header">
					<h2 class="modal-title">{title}</h2>
					<button class="close-btn" onclick={onClose} aria-label="Close">
						<Icon icon="mynaui:x" width="18" />
					</button>
				</div>
			{/if}

			<div class="modal-body">
				{@render children()}
			</div>

			{#if footer && !bare}
				<div class="modal-footer">
					{@render footer()}
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	@reference "../../app.css";

	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: flex-start;
		justify-content: center;
		/* Bottom inset keeps tall modals above the home indicator on phones. */
		padding-top: max(15vh, env(safe-area-inset-top));
		padding-bottom: max(16px, env(safe-area-inset-bottom));
		z-index: var(--z-modal);
		animation: backdrop-fade-in var(--motion-enter) var(--ease-premium);
	}

	@keyframes backdrop-fade-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.modal {
		outline: none;
		width: 100%;
		background: var(--surface);
		/* The modal is portaled to <body>, which sets no text color, so
		   content without its own color would get the browser's black. */
		color: var(--foreground);
		border: 1px solid var(--border);
		border-radius: var(--radius-xl);
		box-shadow: var(--shadow-float);
		overflow: hidden;
		/* It settles down into place and, closing (floatOut), shrinks back
		   toward its top edge as it fades. */
		transform-origin: top center;
		animation: modal-slide-in var(--motion-enter) var(--ease-premium);
		/* A modal taller than the window scrolls its body; the header and
		   footer stay put. Without this, anything past the fold could not be
		   reached at all. */
		display: flex;
		flex-direction: column;
		max-height: 100%;
	}

	.modal-header,
	.modal-footer {
		flex-shrink: 0;
	}

	.max-w-sm { max-width: 360px; }
	.max-w-md { max-width: 480px; }
	.max-w-lg { max-width: 600px; }
	.max-w-xl { max-width: 640px; }
	.bare .modal-body { padding: 0; }

	@keyframes modal-slide-in {
		from {
			opacity: 0;
			transform: translateY(-8px) scale(0.98);
		}
		to {
			opacity: 1;
			transform: translateY(0) scale(1);
		}
	}

	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px;
		border-bottom: 1px solid var(--border);
	}

	.modal-title {
		font-size: 16px;
		font-weight: 500;
		color: var(--foreground);
		margin: 0;
	}

	.close-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--foreground-muted);
		border: none;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium), color var(--motion-hover-out) var(--ease-premium);
	}

	.close-btn:hover {
		transition-duration: var(--motion-hover-in);
		background: var(--surface-overlay);
		color: var(--foreground);
	}

	.modal-body {
		padding: 20px;
		min-height: 0;
		overflow-y: auto;
	}

	.modal-footer {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 10px;
		padding: 16px 20px;
		border-top: 1px solid var(--border);
		background: var(--surface-elevated);
	}

	/* Utility classes for modal content */
	:global(.modal-input) {
		width: 100%;
		padding: 10px 12px;
		font-size: 14px;
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		background: var(--surface);
		color: var(--foreground);
		outline: none;
		transition: border-color 150ms ease;
	}

	:global(.modal-input:focus) {
		border-color: var(--primary);
	}

	:global(.modal-input::placeholder) {
		color: var(--foreground-subtle);
	}

	:global(.modal-label) {
		display: block;
		font-size: 13px;
		font-weight: 500;
		color: var(--foreground-muted);
		margin-bottom: 6px;
	}

	/* The `:global(.modal-btn*)` recipe lived here until 2026-09-16 — a button
	   definition leaking out of one component's <style> and consumed by fifteen
	   buttons in four other files, which is how a modal footer came to be
	   styled by whichever component happened to own the dialog. Its last two
	   consumers (DialogHost, PersonaSettings) now use `Button`, so it is gone.
	   A modal's footer takes the same buttons as the rest of the app. */
</style>
