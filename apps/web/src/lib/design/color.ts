/**
 * Color readings for the /design page (agents/build/design.md): resolve a
 * token to real sRGB through the cascade, and measure WCAG 2.1 contrast.
 */

import { getAvailableThemes, type Theme } from '$lib/utils/theme';

export type RGBA = { r: number; g: number; b: number; a: number };

/** Handles both `rgb()/rgba()` and the `color(srgb r g b / a)` form `color-mix()` computes to. */
export function parseColor(str: string): RGBA | null {
	const s = str.trim();
	let m = /^rgba?\(([^)]+)\)$/i.exec(s);
	if (m) {
		const p = m[1].split(/[,/\s]+/).filter(Boolean).map(Number);
		if (p.length >= 3 && p.slice(0, 3).every((n) => !Number.isNaN(n))) {
			return { r: p[0], g: p[1], b: p[2], a: p.length > 3 && !Number.isNaN(p[3]) ? p[3] : 1 };
		}
		return null;
	}
	m = /^color\(srgb\s+([^)]+)\)$/i.exec(s);
	if (m) {
		const p = m[1].split(/[/\s]+/).filter(Boolean).map(Number);
		if (p.length >= 3 && p.slice(0, 3).every((n) => !Number.isNaN(n))) {
			return { r: p[0] * 255, g: p[1] * 255, b: p[2] * 255, a: p.length > 3 && !Number.isNaN(p[3]) ? p[3] : 1 };
		}
	}
	return null;
}

function luminance(c: RGBA): number {
	const f = (v: number) => {
		const s = Math.min(255, Math.max(0, v)) / 255;
		return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
	};
	return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
}

/** Flatten a translucent color onto an opaque one. */
export function over(fg: RGBA, bg: RGBA): RGBA {
	const a = Math.min(1, Math.max(0, fg.a));
	return { r: fg.r * a + bg.r * (1 - a), g: fg.g * a + bg.g * (1 - a), b: fg.b * a + bg.b * (1 - a), a: 1 };
}

export function contrast(fg: RGBA, bg: RGBA): number {
	const a = luminance(fg);
	const b = luminance(bg);
	return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

export function hex(c: RGBA): string {
	const h = (v: number) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, '0');
	return `#${h(c.r)}${h(c.g)}${h(c.b)}`.toUpperCase();
}

/** An invalid color leaves this behind, which is how a rejected value is detected. */
const SENTINEL = 'rgb(1, 2, 3)';

function resolveColor(probe: HTMLElement, value: string): RGBA | null {
	if (!value) return null;
	probe.style.color = SENTINEL;
	probe.style.color = value;
	const computed = getComputedStyle(probe).color;
	if (computed.replace(/\s+/g, '') === SENTINEL.replace(/\s+/g, '')) return null;
	return parseColor(computed);
}

export type TokenReading = { value: string; rgb: RGBA | null };

/**
 * Read tokens in each theme. It sets `data-theme` on <html>, reads the
 * cascade and puts the attribute back, all in one task, so nothing paints
 * in between. `--name` is read as written; colors are resolved through a
 * hidden probe so `color-mix()` resolves exactly as the browser draws it.
 */
export function readTokens(names: readonly string[]): Record<Theme, Record<string, TokenReading>> {
	const html = document.documentElement;
	const previous = html.getAttribute('data-theme');
	const probe = document.createElement('span');
	probe.style.cssText = 'position:fixed;left:-9999px;top:-9999px;width:0;height:0;';
	document.body.appendChild(probe);

	const out = {} as Record<Theme, Record<string, TokenReading>>;
	// The two themes themes.css defines; Match system and Follow the day resolve to one of them.
	const themes = getAvailableThemes().filter((c): c is Theme => c === 'oxford' || c === 'asgard');
	for (const theme of themes) {
		html.setAttribute('data-theme', theme);
		const cs = getComputedStyle(html);
		const row: Record<string, TokenReading> = {};
		for (const name of names) {
			const value = cs.getPropertyValue(name).trim();
			row[name] = { value, rgb: resolveColor(probe, value ? `var(${name})` : '') };
		}
		out[theme] = row;
	}

	probe.remove();
	if (previous) html.setAttribute('data-theme', previous);
	else html.removeAttribute('data-theme');
	return out;
}
