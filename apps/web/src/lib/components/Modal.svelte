<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import type { Snippet } from "svelte";

	interface Props {
		open?: boolean;
		onClose: () => void;
		title?: string;
		width?: "sm" | "md" | "lg";
		children: Snippet;
		footer?: Snippet;
	}

	let {
		open = false,
		onClose,
		title,
		width = "md",
		children,
		footer,
	}: Props = $props();

	// Portal action - moves element to body
	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		
		return {
			destroy() {
				node.remove();
			}
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
				if (e.key === "Escape") {
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
	}[width]);
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions a11y_no_noninteractive_element_interactions -->
	<div class="modal-backdrop" onclick={handleBackdropClick} role="presentation" use:portal>
		<div class="modal {widthClass}" role="dialog" aria-modal="true" aria-label={title}>
			{#if title}
				<div class="modal-header">
					<h2 class="modal-title">{title}</h2>
					<button class="close-btn" onclick={onClose} aria-label="Close">
						<Icon icon="ri:close-line" width="18" />
					</button>
				</div>
			{/if}

			<div class="modal-body">
				{@render children()}
			</div>

			{#if footer}
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
		padding-top: 15vh;
		z-index: 9999;
		animation: backdrop-fade-in 150ms ease-out;
	}

	@keyframes backdrop-fade-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.modal {
		width: 100%;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 12px;
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2);
		overflow: hidden;
		animation: modal-slide-in 150ms ease-out;
	}

	.max-w-sm { max-width: 360px; }
	.max-w-md { max-width: 480px; }
	.max-w-lg { max-width: 600px; }

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
		border-radius: 6px;
		background: transparent;
		color: var(--foreground-muted);
		border: none;
		cursor: pointer;
		transition: all 150ms ease;
	}

	.close-btn:hover {
		background: var(--surface-overlay);
		color: var(--foreground);
	}

	.modal-body {
		padding: 20px;
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
		border-radius: 8px;
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

	:global(.modal-btn) {
		padding: 8px 16px;
		font-size: 13px;
		font-weight: 500;
		border-radius: 6px;
		cursor: pointer;
		transition: all 150ms ease;
	}

	:global(.modal-btn-primary) {
		background: var(--primary);
		color: white;
		border: none;
	}

	:global(.modal-btn-primary:hover) {
		opacity: 0.9;
	}

	:global(.modal-btn-primary:disabled) {
		opacity: 0.5;
		cursor: not-allowed;
	}

	:global(.modal-btn-secondary) {
		background: transparent;
		color: var(--foreground-muted);
		border: 1px solid var(--border);
	}

	:global(.modal-btn-secondary:hover) {
		background: var(--surface-overlay);
		color: var(--foreground);
	}
</style>
