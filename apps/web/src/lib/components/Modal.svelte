<script lang="ts">
	/**
	 * Modal - Universal modal component
	 * Reusable dialog with backdrop, escape-to-close, and body scroll lock
	 */
	import type { Snippet } from "svelte";

	interface Props {
		open: boolean;
		onClose: () => void;
		title?: string;
		subtitle?: string;
		size?: "sm" | "md" | "lg";
		children: Snippet;
		footer?: Snippet;
	}

	let {
		open = false,
		onClose,
		title,
		subtitle,
		size = "md",
		children,
		footer,
	}: Props = $props();

	// Size mappings
	const sizeClasses = {
		sm: "max-w-sm",
		md: "max-w-md",
		lg: "max-w-lg",
	};

	// Handle escape key
	function handleKeydown(e: KeyboardEvent) {
		if (e.key === "Escape" && open) {
			onClose();
		}
	}

	// Handle backdrop click
	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	$effect(() => {
		if (open) {
			document.addEventListener("keydown", handleKeydown);
			// Prevent body scroll when modal is open
			document.body.style.overflow = "hidden";
		} else {
			document.body.style.overflow = "";
		}

		return () => {
			document.removeEventListener("keydown", handleKeydown);
			document.body.style.overflow = "";
		};
	});
</script>

{#if open}
	<div
		class="modal-backdrop"
		onclick={handleBackdropClick}
		role="presentation"
	>
		<div
			class="modal {sizeClasses[size]}"
			role="dialog"
			aria-modal="true"
			aria-labelledby={title ? "modal-title" : undefined}
		>
			{#if title}
				<header class="modal-header">
					<h2 id="modal-title" class="modal-title">
						{title}
					</h2>
					{#if subtitle}
						<p class="modal-subtitle">{subtitle}</p>
					{/if}
				</header>
			{/if}

			<div class="modal-content">
				{@render children()}
			</div>

			{#if footer}
				<footer class="modal-footer">
					{@render footer()}
				</footer>
			{/if}
		</div>
	</div>
{/if}

<style>
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
		animation: fadeIn 150ms ease-out;
	}

	@keyframes fadeIn {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.modal {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: 12px;
		width: 100%;
		padding: 32px;
		margin: 24px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12);
		animation: scaleIn 150ms ease-out;
	}

	@keyframes scaleIn {
		from {
			opacity: 0;
			transform: scale(0.96);
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}

	.modal-header {
		margin-bottom: 16px;
	}

	.modal-title {
		font-family: var(--font-serif);
		font-size: 22px;
		font-weight: 400;
		letter-spacing: -0.01em;
		color: var(--foreground);
		margin: 0;
	}

	.modal-subtitle {
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--foreground-muted);
		margin: 8px 0 0 0;
		line-height: 1.5;
	}

	.modal-footer {
		display: flex;
		justify-content: flex-end;
		gap: 16px;
		margin-top: 24px;
	}
</style>
