/**
 * An applet block before its face is mounted: the height it reserves, and
 * the box that holds its place (and prints, when the frame never mounted:
 * a box naming the applet, as the mounted one prints).
 */

import { getApplet } from '$lib/api/client';

/** The frame's height when the block says none, in CSS pixels. */
export const DEFAULT_APPLET_HEIGHT = 420;

const names = new Map<string, Promise<string | null>>();

/** An applet's name, asked once per applet however many blocks show it; null when it cannot be read. */
function appletName(ref: string): Promise<string | null> {
	let name = names.get(ref);
	if (!name) {
		name = getApplet(ref).then(
			(a) => a.name || null,
			() => {
				names.delete(ref);
				return null;
			},
		);
		names.set(ref, name);
	}
	return name;
}

/** The placeholder: the applet's id until its name is read, then its name. */
export function appletPlaceholder(
	attrs: Record<string, unknown>,
	nameOf: (ref: string) => Promise<string | null> = appletName,
): HTMLElement {
	const ref = String(attrs.ref ?? '');
	const box = document.createElement('div');
	box.className = 'doc-applet-placeholder';
	box.style.height = `${Number(attrs.height ?? DEFAULT_APPLET_HEIGHT)}px`;
	const label = document.createElement('span');
	label.className = 'doc-applet-placeholder-name';
	label.textContent = ref;
	box.append(label);
	if (ref) {
		void nameOf(ref).then(
			(name) => {
				if (name) label.textContent = name;
			},
			() => {},
		);
	}
	return box;
}
