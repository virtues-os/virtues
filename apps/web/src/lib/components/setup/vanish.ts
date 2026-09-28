/**
 * Vanish — text that dissolves into its own ink (after Rauno Freiberg's
 * vanishing input).
 *
 * The text is drawn once to a canvas laid exactly over the field, in the
 * field's own type; its pixels become particles; a sweep runs from the end
 * of the text to its start, and every particle the sweep has passed drifts,
 * rises a little and shrinks to nothing. The field underneath is free the
 * moment this starts: the caller clears or replaces the text at once, and
 * the old words go away on their own.
 *
 * Canvas, not DOM: a word at display size is a few thousand particles, and
 * only a canvas moves that many at 60fps. With reduced motion it resolves at
 * once and draws nothing.
 */

interface Particle {
	x: number;
	y: number;
	r: number;
	color: string;
}

/** Sample every Nth CSS pixel: dense enough to read as the word, light
 *  enough to animate at display size. */
const STEP = 2;
/** How far the canvas reaches past the field, for the drift. */
const MARGIN = 48;

export function vanish(field: HTMLInputElement | HTMLTextAreaElement, text = field.value): Promise<void> {
	const reduced =
		typeof window === 'undefined' || !!window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
	if (reduced || !text.trim()) return Promise.resolve();

	const rect = field.getBoundingClientRect();
	const cs = getComputedStyle(field);
	const dpr = window.devicePixelRatio || 1;
	const w = rect.width + MARGIN * 2;
	const h = rect.height + MARGIN * 2;

	const canvas = document.createElement('canvas');
	canvas.width = Math.ceil(w * dpr);
	canvas.height = Math.ceil(h * dpr);
	Object.assign(canvas.style, {
		position: 'fixed',
		left: `${rect.left - MARGIN}px`,
		top: `${rect.top - MARGIN}px`,
		width: `${w}px`,
		height: `${h}px`,
		pointerEvents: 'none',
		zIndex: '70',
	});
	const ctx = canvas.getContext('2d', { willReadFrequently: true });
	if (!ctx) return Promise.resolve();
	document.body.appendChild(canvas);
	ctx.scale(dpr, dpr);

	// The field's own type, placed where the field puts its text.
	ctx.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
	ctx.fillStyle = cs.color;
	ctx.textBaseline = 'middle';
	const padL = parseFloat(cs.paddingLeft) || 0;
	const padR = parseFloat(cs.paddingRight) || 0;
	const textW = ctx.measureText(text).width;
	const inner = rect.width - padL - padR;
	const align = cs.textAlign;
	const x0 =
		MARGIN +
		padL +
		(align === 'center' ? (inner - textW) / 2 : align === 'right' || align === 'end' ? inner - textW : 0);
	ctx.fillText(text, x0, MARGIN + rect.height / 2);

	// Pixels → particles.
	const img = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
	const particles: Particle[] = [];
	const stride = Math.max(1, Math.round(STEP * dpr));
	for (let py = 0; py < canvas.height; py += stride) {
		for (let px = 0; px < canvas.width; px += stride) {
			const i = (py * canvas.width + px) * 4;
			const a = img[i + 3];
			if (a > 110) {
				particles.push({
					x: px / dpr,
					y: py / dpr,
					r: STEP * 0.75,
					color: `rgba(${img[i]}, ${img[i + 1]}, ${img[i + 2]}, ${a / 255})`,
				});
			}
		}
	}

	return new Promise((resolve) => {
		// The sweep starts at the end of the text and runs to its start.
		let sweep = x0 + textW;
		const speed = Math.max(6, textW / 26);
		const frame = () => {
			sweep -= speed;
			ctx.clearRect(0, 0, w, h);
			let alive = 0;
			for (const p of particles) {
				if (p.r <= 0) continue;
				alive++;
				if (p.x > sweep) {
					p.x += Math.random() * 2.2 - 0.6;
					p.y += Math.random() * 2 - 1.3;
					p.r -= 0.045 * Math.random() + 0.012;
				}
				if (p.r > 0) {
					ctx.fillStyle = p.color;
					ctx.fillRect(p.x, p.y, p.r * 1.6, p.r * 1.6);
				}
			}
			if (alive > 0) requestAnimationFrame(frame);
			else {
				canvas.remove();
				resolve();
			}
		};
		requestAnimationFrame(frame);
	});
}
