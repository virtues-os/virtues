/**
 * Three equal masses under Newtonian gravity, G = m = 1.
 *
 * The starting state is the figure-eight choreography (Chenciner and
 * Montgomery, 2000; numerics from Simó): three bodies chasing one another
 * round a single figure-eight, period ≈ 6.3259. It is a real solution and it
 * is stable, so left alone it holds — and any kick sends the system chaotic,
 * which is the point of letting someone throw a body.
 *
 * Integration is kick-drift-kick leapfrog (symplectic, so energy does not
 * creep over a long watch) with a step that shrinks as the closest pair
 * closes in, plus a small softening so a near-collision cannot blow up.
 */

export type Vec3 = [number, number, number];

export interface Bodies {
	pos: [Vec3, Vec3, Vec3];
	vel: [Vec3, Vec3, Vec3];
}

export const FIGURE_EIGHT_PERIOD = 6.32591398;

const SOFTENING_SQ = 1e-4;
const MAX_STEP = 0.002;

export function figureEight(): Bodies {
	const x = 0.97000436;
	const y = -0.24308753;
	const vx = -0.93240737;
	const vy = -0.86473146;
	return {
		pos: [
			[x, y, 0],
			[0, 0, 0],
			[-x, -y, 0],
		],
		vel: [
			[-vx / 2, -vy / 2, 0],
			[vx, vy, 0],
			[-vx / 2, -vy / 2, 0],
		],
	};
}

function accelerations(pos: Bodies["pos"]): { acc: [Vec3, Vec3, Vec3]; minDist: number } {
	const acc: [Vec3, Vec3, Vec3] = [
		[0, 0, 0],
		[0, 0, 0],
		[0, 0, 0],
	];
	let minDist = Infinity;
	for (let i = 0; i < 3; i++) {
		for (let j = i + 1; j < 3; j++) {
			const dx = pos[j][0] - pos[i][0];
			const dy = pos[j][1] - pos[i][1];
			const dz = pos[j][2] - pos[i][2];
			const r2 = dx * dx + dy * dy + dz * dz;
			minDist = Math.min(minDist, Math.sqrt(r2));
			const s = r2 + SOFTENING_SQ;
			const inv = 1 / (s * Math.sqrt(s));
			acc[i][0] += dx * inv;
			acc[i][1] += dy * inv;
			acc[i][2] += dz * inv;
			acc[j][0] -= dx * inv;
			acc[j][1] -= dy * inv;
			acc[j][2] -= dz * inv;
		}
	}
	return { acc, minDist };
}

/** Advance the system by `duration` time units, in place. */
export function advance(b: Bodies, duration: number): void {
	let left = duration;
	let { acc, minDist } = accelerations(b.pos);
	// A cap on work per call: a very close pass would otherwise stall a frame.
	for (let n = 0; left > 1e-12 && n < 20000; n++) {
		const dt = Math.min(left, MAX_STEP, 0.02 * minDist ** 1.5 + 1e-5);
		for (let i = 0; i < 3; i++) {
			for (let k = 0; k < 3; k++) {
				b.vel[i][k] += 0.5 * dt * acc[i][k];
				b.pos[i][k] += dt * b.vel[i][k];
			}
		}
		({ acc, minDist } = accelerations(b.pos));
		for (let i = 0; i < 3; i++) {
			for (let k = 0; k < 3; k++) b.vel[i][k] += 0.5 * dt * acc[i][k];
		}
		left -= dt;
	}
}

/** Total energy, kinetic plus (softened) potential. */
export function energy(b: Bodies): number {
	let e = 0;
	for (let i = 0; i < 3; i++) {
		const [vx, vy, vz] = b.vel[i];
		e += 0.5 * (vx * vx + vy * vy + vz * vz);
	}
	for (let i = 0; i < 3; i++) {
		for (let j = i + 1; j < 3; j++) {
			const dx = b.pos[j][0] - b.pos[i][0];
			const dy = b.pos[j][1] - b.pos[i][1];
			const dz = b.pos[j][2] - b.pos[i][2];
			e -= 1 / Math.sqrt(dx * dx + dy * dy + dz * dz + SOFTENING_SQ);
		}
	}
	return e;
}

export function centerOfMass(b: Bodies): Vec3 {
	return [
		(b.pos[0][0] + b.pos[1][0] + b.pos[2][0]) / 3,
		(b.pos[0][1] + b.pos[1][1] + b.pos[2][1]) / 3,
		(b.pos[0][2] + b.pos[1][2] + b.pos[2][2]) / 3,
	];
}
