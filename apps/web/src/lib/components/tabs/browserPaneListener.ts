/**
 * When the shell shows a page in the Browser (the assistant's browser_open, a
 * handoff, or "Log in to X"), open the Browser tab beside the current view, or
 * bring the one already open to the front of its pane. A link you click opens
 * the same way (`webLinks.ts`).
 *
 * Also feeds `browserAgent` with who is driving and the assistant's steps, for
 * the Browser tab's bar and step log.
 */
import { browserAgent } from '$lib/stores/browserAgent.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { browserLabel } from '$lib/tabs/registry';
import { canBrowserPane, onBrowserAgent, onBrowserEvent } from '$lib/tauri/bridge';

export async function listenForBrowserOpen(): Promise<() => void> {
	if (!(await canBrowserPane())) return () => {};
	const offAgent = await onBrowserAgent(
		(state) => browserAgent.setState(state),
		(step) => browserAgent.addStep(step)
	);
	const offOpen = await onBrowserEvent('browser:open', showBrowserTab);
	return () => {
		offAgent();
		offOpen();
	};
}

/**
 * Show the Browser tab on `url`, the page the shell is showing. A Browser tab
 * already open changes page in place and comes to the front of its pane, so
 * the owner's focus stays where it is; otherwise one opens beside the
 * current view.
 */
export function showBrowserTab(url: string): void {
	const route = `/browser?url=${encodeURIComponent(url)}`;
	const label = browserLabel(url);
	const open = windowShellStore.findTab((t) => t.type === 'browser');
	if (open) {
		windowShellStore.updateTab(open.tab.id, { route, label });
		windowShellStore.setActiveTabInPane(open.tab.id, open.paneId as 'left' | 'right');
		return;
	}
	windowShellStore.openAside({ type: 'browser', route, label, icon: 'mynaui:globe' });
}
