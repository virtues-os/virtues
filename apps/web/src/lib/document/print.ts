/**
 * Printing a block page: the page alone, on as many sheets as it needs. The
 * page's scroller (`root`) and the boxes around it are marked, and the rest
 * of the app is set aside (`data-print-root`, `-chain`, `-off`, which the
 * page's print CSS reads). The page's own chrome marks itself
 * `data-print="hide"`; the rest of the app does not know it is being
 * printed, so it is set aside from here.
 *
 * The marks go on at `beforeprint`, so the browser's own Print (⌘P, File >
 * Print, the share sheet) prints the page as the toolbar's Print does, and
 * come off at `afterprint`. Only one page marks, and only one in view: a
 * page in a hidden tab stays mounted, and two pages can sit side by side.
 *
 * In the Mac app `window.print` is Tauri's: a command (`core:webview:allow-print`)
 * that runs WebKit's own print operation and answers with a promise. That
 * operation is not known to fire `beforeprint`, so the page's own Print
 * marks before it prints, and the marks come off at `afterprint` or, failing
 * that, at the first use of the window once the print has been handed over.
 * The iOS app has no print command at all (`canPrint`).
 */

import { toast } from 'svelte-sonner';
import { isIOS } from '$lib/utils/platform';

/** Whether this app prints: the iOS app's `window.print` is a command its shell does not have. */
export const canPrint = !isIOS;

/** Set everything but `root` and the boxes around it aside for printing. Returns the undo. */
export function markForPrint(root: Element): () => void {
	const marked: Element[] = [];
	root.setAttribute('data-print-root', '');
	marked.push(root);
	for (let el: Element = root; el.parentElement && el !== document.body; el = el.parentElement) {
		const parent = el.parentElement;
		parent.setAttribute('data-print-chain', '');
		marked.push(parent);
		for (const sibling of Array.from(parent.children)) {
			if (sibling === el) continue;
			sibling.setAttribute('data-print-off', '');
			marked.push(sibling);
		}
	}
	return () => {
		for (const el of marked) {
			el.removeAttribute('data-print-root');
			el.removeAttribute('data-print-chain');
			el.removeAttribute('data-print-off');
		}
	};
}

/** Whether `el` is drawn: a page in a hidden tab, kept mounted, is not. */
function drawn(el: Element): boolean {
	if (!el.isConnected) return false;
	const check = (el as Element & { checkVisibility?: () => boolean }).checkVisibility;
	return typeof check === 'function' ? check.call(el) : el.getClientRects().length > 0;
}

type Root = () => Element | null;

/** The pages that print alone in one window, and which of them prints. */
interface Printing {
	roots: Root[];
	/** The page whose own Print ran (`printThis`). */
	asked: Element | null;
	/** The page last clicked or focused in. */
	last: Root | null;
	undo: (() => void) | null;
	/** The print was handed over: the marks come off at the next use of the window. */
	handedOver: boolean;
	/** Take the marks off. */
	done: () => void;
	stop: () => void;
}

const windows = new Map<Window, Printing>();

/**
 * The one page a print prints: the page whose own Print ran, else the one
 * the focus is in, else the one last used, else the last opened; only a
 * page that is drawn. Every open page listens (hidden tabs stay mounted),
 * and each would set the others, and whatever is in view, aside.
 */
function chosen(p: Printing): Element | null {
	const shown = p.roots.map((root) => ({ root, el: root() })).filter((r): r is { root: Root; el: Element } => !!r.el && drawn(r.el));
	if (!shown.length) return null;
	const focus = document.activeElement;
	return (
		shown.find((r) => r.el === p.asked)?.el ??
		shown.find((r) => !!focus && r.el.contains(focus))?.el ??
		shown.find((r) => r.root === p.last)?.el ??
		shown[shown.length - 1].el
	);
}

function printingIn(target: Window): Printing {
	const had = windows.get(target);
	if (had) return had;
	const before = () => {
		p.undo?.();
		const el = chosen(p);
		p.undo = el ? markForPrint(el) : null;
	};
	const after = () => {
		p.undo?.();
		p.undo = null;
		p.asked = null;
		p.handedOver = false;
	};
	const used = (e: Event) => {
		if (p.handedOver) after();
		const at = e.target as Node | null;
		const root = p.roots.find((r) => !!at && !!r()?.contains(at));
		if (root) p.last = root;
	};
	const p: Printing = {
		roots: [],
		asked: null,
		last: null,
		undo: null,
		handedOver: false,
		done: after,
		stop: () => {
			after();
			target.removeEventListener('beforeprint', before);
			target.removeEventListener('afterprint', after);
			target.removeEventListener('pointerdown', used, true);
			target.removeEventListener('focusin', used, true);
			target.removeEventListener('keydown', used, true);
			windows.delete(target);
		},
	};
	target.addEventListener('beforeprint', before);
	target.addEventListener('afterprint', after);
	target.addEventListener('pointerdown', used, true);
	target.addEventListener('focusin', used, true);
	target.addEventListener('keydown', used, true);
	windows.set(target, p);
	return p;
}

/**
 * While this holds, a print of the window prints one page alone: `root()`
 * when it is the page in view (`chosen`). Returns the release.
 */
export function printAlone(root: Root, target: Window = window): () => void {
	const p = printingIn(target);
	p.roots.push(root);
	return () => {
		p.roots = p.roots.filter((r) => r !== root);
		if (p.last === root) p.last = null;
		if (!p.roots.length) p.stop();
	};
}

/**
 * A page's own Print: the window prints `root`, a page that `printAlone`
 * holds, marked before the print starts. A print that fails is said.
 */
export function printThis(root: Element, target: Window = window): void {
	const p = printingIn(target);
	p.asked = root;
	p.undo?.();
	p.undo = markForPrint(root);
	const failed = (error: unknown) => {
		p.done();
		toast.error("Couldn't print this page", { description: error instanceof Error ? error.message : String(error) });
	};
	let result: unknown;
	try {
		result = (target.print as () => unknown)();
	} catch (error) {
		failed(error);
		return;
	}
	if (result instanceof Promise) {
		result.then(
			() => {
				if (p.undo) p.handedOver = true;
			},
			failed,
		);
	}
}
