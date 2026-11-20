/**
 * Lighting the row a Settings link asked for, once its page has mounted.
 *
 * Kept apart from `focus.ts` because the search list imports that module at
 * launch, and only Settings pages need this one.
 */
import { tick } from 'svelte';
import { reducedMotion } from '$lib/stores/motion.svelte';
import { lightUp } from '$lib/utils/lit';
import { takePendingSetting } from './focus';

/**
 * Scroll to the row a link asked for and light it, once. Call it when the
 * page mounts; it returns the cleanup. The row is the element with id
 * `setting-<id>`, or for an empty marker inside a block, the block.
 *
 * Some rows draw only once their values load, and the tab restores its own
 * scroll position once the route settles. So it waits for the row, scrolls
 * to it, and once more, at once, if it still isn't on screen.
 */
export function lightPendingSetting(): () => void {
	const settingId = takePendingSetting();
	if (!settingId) return () => {};
	let timer: ReturnType<typeof setTimeout> | undefined;
	const started = Date.now();
	const target = (): HTMLElement | null => {
		const el = document.getElementById(`setting-${settingId}`);
		if (!el) return null;
		return el.offsetHeight > 0 ? el : (el.closest<HTMLElement>('.block') ?? el.parentElement);
	};
	const find = async () => {
		await tick();
		const el = target();
		if (!el) {
			if (Date.now() - started < 3000) timer = setTimeout(find, 150);
			return;
		}
		el.scrollIntoView({ behavior: reducedMotion() ? 'auto' : 'smooth', block: 'center' });
		lightUp(el);
		timer = setTimeout(() => {
			const box = el.getBoundingClientRect();
			if (box.top < 0 || box.bottom > window.innerHeight) el.scrollIntoView({ block: 'center' });
		}, 650);
	};
	timer = setTimeout(find, 250);
	return () => clearTimeout(timer);
}
