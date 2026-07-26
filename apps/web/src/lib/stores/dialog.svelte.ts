/**
 * Promise-based confirm/prompt dialogs.
 *
 * Replaces `window.confirm()` / `window.prompt()`, which are unreliable in the
 * Tauri/WKWebView shell — `prompt()` is a no-op there, and a swallowed
 * `confirm()` silently cancels whatever it was gating, so destructive actions
 * appear to do nothing. These render through the app's own Modal instead.
 *
 * Usage (anywhere, including plain .ts):
 *     if (!(await confirmAction({ title: 'Delete page?', danger: true }))) return;
 *     const name = await promptText({ title: 'Notebook name' });
 *
 * A single <DialogHost /> in the app layout renders whatever is pending.
 */

export interface ConfirmOptions {
	title: string;
	/** Optional supporting line. Plain text — say what survives, not just what dies. */
	body?: string;
	confirmLabel?: string;
	cancelLabel?: string;
	/** Red confirm button, for destructive actions. */
	danger?: boolean;
}

export interface PromptOptions {
	title: string;
	body?: string;
	placeholder?: string;
	initialValue?: string;
	confirmLabel?: string;
	cancelLabel?: string;
}

type PendingConfirm = ConfirmOptions & {
	kind: 'confirm';
	resolve: (ok: boolean) => void;
};
type PendingPrompt = PromptOptions & {
	kind: 'prompt';
	resolve: (value: string | null) => void;
};

export class DialogStore {
	pending = $state<PendingConfirm | PendingPrompt | null>(null);

	/** Ask for confirmation. Resolves true only on an explicit confirm. */
	confirm(options: ConfirmOptions): Promise<boolean> {
		// A second dialog while one is open would strand the first promise;
		// resolve the incumbent as cancelled rather than leaking it.
		this.settle();
		return new Promise((resolve) => {
			this.pending = { kind: 'confirm', ...options, resolve };
		});
	}

	/** Ask for a line of text. Resolves null on cancel or an empty entry. */
	prompt(options: PromptOptions): Promise<string | null> {
		this.settle();
		return new Promise((resolve) => {
			this.pending = { kind: 'prompt', ...options, resolve };
		});
	}

	/** Confirm the open dialog with `value` (text dialogs) or true. */
	accept(value?: string): void {
		const p = this.pending;
		if (!p) return;
		this.pending = null;
		if (p.kind === 'confirm') p.resolve(true);
		else p.resolve(value?.trim() ? value.trim() : null);
	}

	/** Dismiss the open dialog — cancel, backdrop click, or Escape. */
	cancel(): void {
		this.settle();
	}

	private settle(): void {
		const p = this.pending;
		if (!p) return;
		this.pending = null;
		if (p.kind === 'confirm') p.resolve(false);
		else p.resolve(null);
	}
}

export const dialogStore = new DialogStore();

/** Shorthand: `if (!(await confirmAction({ title: '…' }))) return;` */
export const confirmAction = (options: ConfirmOptions): Promise<boolean> =>
	dialogStore.confirm(options);

/** Shorthand: `const name = await promptText({ title: '…' });` */
export const promptText = (options: PromptOptions): Promise<string | null> =>
	dialogStore.prompt(options);
