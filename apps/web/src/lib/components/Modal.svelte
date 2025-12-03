<script lang="ts">
	/**
	 * Modal - Universal modal component
	 * Supports default and manuscript variants
	 */
	import type { Snippet } from "svelte";

	interface Props {
		open: boolean;
		onClose: () => void;
		title?: string;
		subtitle?: string;
		size?: "sm" | "md" | "lg";
		variant?: "default" | "manuscript";
		children: Snippet;
		footer?: Snippet;
	}

	let {
		open = false,
		onClose,
		title,
		subtitle,
		size = "md",
		variant = "default",
		children,
		footer,
	}: Props = $props();

	const isManuscript = variant === "manuscript";

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
		class:manuscript={isManuscript}
		onclick={handleBackdropClick}
		role="presentation"
	>
		<div
			class="modal {sizeClasses[size]}"
			class:manuscript={isManuscript}
			role="dialog"
			aria-modal="true"
			aria-labelledby={title ? "modal-title" : undefined}
		>
			{#if title}
				<header class="modal-header">
					<h2 id="modal-title" class="modal-title" class:manuscript={isManuscript}>
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
		background: rgba(0, 0, 0, 0.4);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
		animation: fadeIn 150ms ease-out;
	}

	.modal-backdrop.manuscript {
		background: rgba(0, 0, 0, 0.5);
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
		border-radius: 8px;
		width: 100%;
		padding: 32px;
		margin: 24px;
		animation: scaleIn 150ms ease-out;
	}

	.modal.manuscript {
		border-radius: 0;
		border-color: var(--foreground);
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
		font-size: 20px;
		font-weight: 500;
		color: var(--foreground);
		margin: 0;
	}

	.modal-title.manuscript {
		font-size: 22px;
		font-weight: 400;
		letter-spacing: -0.01em;
	}

	.modal-subtitle {
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--foreground-muted);
		margin: 8px 0 0 0;
		line-height: 1.5;
	}

	.modal-content {
		/* Content styles handled by children */
	}

	.modal-footer {
		display: flex;
		justify-content: flex-end;
		gap: 16px;
		margin-top: 24px;
	}
</style>
