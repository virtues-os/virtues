/**
 * A spring as a CSS easing, for the Web Animations API and CSS, which take a
 * duration and a curve rather than physics.
 *
 * The spring is simulated once, from rest at 0 to rest at 1, and written as
 * a `linear()` curve through its samples; the duration is how long it takes
 * to settle. The curve can pass 1 before it settles, which is the overshoot
 * that makes a spring read as one. An engine without `linear()` gets a
 * plain ease-out of the same length.
 */
export interface SpringEasing {
	/** Milliseconds until the spring settles. */
	duration: number;
	easing: string;
}

const EASE_OUT = 'cubic-bezier(0.16, 1, 0.3, 1)';

export function springEasing(stiffness: number, damping: number, mass = 1): SpringEasing {
	const dt = 1 / 120;
	const samples: number[] = [];
	let x = 0;
	let v = 0;
	let time = 0;
	while (time < 3) {
		const a = (-stiffness * (x - 1) - damping * v) / mass;
		v += a * dt;
		x += v * dt;
		time += dt;
		samples.push(x);
		if (time > 0.1 && Math.abs(x - 1) < 0.001 && Math.abs(v) < 0.01) break;
	}
	const duration = Math.round(time * 1000);
	const supported = typeof CSS !== 'undefined' && CSS.supports('animation-timing-function', 'linear(0, 1)');
	if (!supported) return { duration, easing: EASE_OUT };
	const step = Math.max(1, Math.floor(samples.length / 60));
	const points = samples.filter((_, i) => i % step === 0).map((p) => +p.toFixed(4));
	points[points.length - 1] = 1;
	return { duration, easing: `linear(0, ${points.join(', ')})` };
}
