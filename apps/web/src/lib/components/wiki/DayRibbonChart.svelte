<script lang="ts">
	import type { DayEvent } from '$lib/wiki/types';
	import { onMount, onDestroy } from 'svelte';
	import {
		Scene,
		WebGLRenderer,
		PerspectiveCamera,
		BufferGeometry,
		Float32BufferAttribute,
		LineSegments,
		LineBasicMaterial,
		Color,
		Vector3,
		SphereGeometry,
		MeshBasicMaterial,
		Mesh,
		Line,
		Group,
		AdditiveBlending,
	} from 'three';

	interface Props {
		events: DayEvent[];
		timezone: string | null;
		hoveredEventId: string | null;
		onhover: (id: string | null) => void;
	}

	let { events, timezone, hoveredEventId, onhover }: Props = $props();

	let canvasEl = $state<HTMLCanvasElement | null>(null);
	let containerEl = $state<HTMLDivElement | null>(null);

	let renderer: WebGLRenderer | null = null;
	let scene: Scene | null = null;
	let camera: PerspectiveCamera | null = null;
	let resizeObserver: ResizeObserver | null = null;
	let themeObserver: MutationObserver | null = null;

	// Per-event highlight state (geometry + material refs for animation)
	interface HighlightState {
		group: Group;
		coreMat: LineBasicMaterial;
		glowMats: LineBasicMaterial[];
		dot: Mesh;
		halo: Mesh;
		haloMat: MeshBasicMaterial;
	}
	let highlightStates = new Map<string, HighlightState>();
	let activeHighlightId: string | null = null;
	let hoverIntensity = 0;
	let hoverTarget = 0;
	const HOVER_SPEED = 7; // per second (~140ms to full)

	/** Smoothstep: gentle ease in+out for hover */
	function smoothstep(t: number): number {
		return t * t * (3 - 2 * t);
	}

	function applyHoverIntensity(state: HighlightState, t: number) {
		const et = smoothstep(t);
		state.coreMat.opacity = et;
		for (const m of state.glowMats) m.opacity = 0.15 * et;
		const s = Math.max(0.001, et);
		state.dot.scale.setScalar(s);
		state.halo.scale.setScalar(s);
		state.haloMat.opacity = 0.2 * et;
	}

	// Rotation state — data group rotates around the center line (X axis)
	let dataGroup: Group | null = null;
	let animFrameId: number | null = null;
	let isDragging = $state(false);
	let dragStartY = 0;
	let dragStartRotation = 0;
	let currentRotation = 0;

	// Ease-back animation state
	let easeBackStart = 0;       // rotation at start of ease-back
	let easeBackTime = 0;        // elapsed time in ease-back (seconds)
	let isEasingBack = false;
	const EASE_BACK_DURATION = 0.6; // seconds

	// Box dimensions
	const BOX_W = 10; // X extent (left to right)
	const BOX_H = 4;  // Y extent (bottom to top)
	const BOX_D = 4;  // Z extent (front to back)
	const GRID = 8;    // 8x8 grid divisions

	const FOV = 35;
	const BACK_GRID_X = 24; // one column per hour

	// Day-relative scaling: same scale for both axes so the shape is honest.
	// ±0.5 deviation from day mean fills the box on each axis.
	const Y_SCALE = BOX_H / 2 / 0.5; // = 4.0
	const Z_SCALE = BOX_D / 2 / 0.5; // = 4.0

	// Legend metrics (updated on each build)
	let activationArea = $state(0);
	let entropyArea = $state(0);
	const BACK_GRID_Y = 8;

	function getThemeColors() {
		const style = getComputedStyle(document.documentElement);
		return {
			foreground: style.getPropertyValue('--color-foreground').trim() || '#e8e8f0',
			foregroundSubtle: style.getPropertyValue('--color-foreground-subtle').trim() || '#707088',
			primary: style.getPropertyValue('--color-primary').trim() || '#3b82f6',
		};
	}

	// ── Grid face builder ───────────────────────────────────────────────
	// Builds a grid of lines on a rectangular face.
	// axis1/axis2 define the two axes, steps1/steps2 define divisions per axis.
	function buildGridFace(
		origin: Vector3,
		axis1: Vector3,
		axis2: Vector3,
		size1: number,
		size2: number,
		steps1: number,
		steps2: number,
	): BufferGeometry {
		const positions: number[] = [];
		const step1 = size1 / steps1;
		const step2 = size2 / steps2;

		// Lines along axis1 (one per step2 division)
		for (let i = 0; i <= steps2; i++) {
			const offset = i * step2;
			const start = origin.clone().addScaledVector(axis2, offset);
			const end = start.clone().addScaledVector(axis1, size1);
			positions.push(start.x, start.y, start.z, end.x, end.y, end.z);
		}

		// Lines along axis2 (one per step1 division)
		for (let i = 0; i <= steps1; i++) {
			const offset = i * step1;
			const start = origin.clone().addScaledVector(axis1, offset);
			const end = start.clone().addScaledVector(axis2, size2);
			positions.push(start.x, start.y, start.z, end.x, end.y, end.z);
		}

		const geo = new BufferGeometry();
		geo.setAttribute('position', new Float32BufferAttribute(positions, 3));
		return geo;
	}

	// ── Axis lines ──────────────────────────────────────────────────────

	function buildAxisLines(): BufferGeometry {
		const positions: number[] = [];
		const xMin = -BOX_W / 2;
		const xMax = BOX_W / 2;
		const backZ = -BOX_D / 2;

		// X axis (time) — back-bottom edge, left to right
		positions.push(xMin, 0, backZ, xMax, 0, backZ);
		// Y axis (activation) — back-left vertical edge, bottom to top
		positions.push(xMin, 0, backZ, xMin, BOX_H, backZ);
		// Z axis (distinctness) — bottom-left edge, back to front
		positions.push(xMin, 0, backZ, xMin, 0, BOX_D / 2);

		const geo = new BufferGeometry();
		geo.setAttribute('position', new Float32BufferAttribute(positions, 3));
		return geo;
	}

	// ── Center line from left face to right face ────────────────────────

	function buildCenterLine(): BufferGeometry {
		const positions: number[] = [];
		const xMin = -BOX_W / 2;
		const xMax = BOX_W / 2;
		const yCenter = BOX_H / 2;
		const zCenter = 0; // center of depth

		positions.push(xMin, yCenter, zCenter, xMax, yCenter, zCenter);

		const geo = new BufferGeometry();
		geo.setAttribute('position', new Float32BufferAttribute(positions, 3));
		return geo;
	}

	// ── Data mapping ────────────────────────────────────────────────────

	function getHourOfDay(date: Date | string): number {
		const d = typeof date === 'string' ? new Date(date) : date;
		if (timezone) {
			const fmt = new Intl.DateTimeFormat('en-US', {
				timeZone: timezone,
				hourCycle: 'h23',
				hour: '2-digit',
				minute: '2-digit',
			});
			const parts = fmt.formatToParts(d);
			const h = parseInt(parts.find((p) => p.type === 'hour')?.value || '0');
			const m = parseInt(parts.find((p) => p.type === 'minute')?.value || '0');
			return h + m / 60;
		}
		return d.getHours() + d.getMinutes() / 60;
	}

	function buildDataPoints(): { points: Vector3[]; eventIds: string[] } {
		if (!events || events.length === 0) return { points: [], eventIds: [] };

		// Sort by start time so the line always moves left → right
		const sorted = [...events].sort(
			(a, b) => new Date(a.startTime).getTime() - new Date(b.startTime).getTime(),
		);

		// ── First pass: compute day means ────────────────────────────
		let sumActivation = 0, countActivation = 0;
		let sumEntropy = 0, countEntropy = 0;

		for (const event of sorted) {
			if (event.w6hActivation) {
				sumActivation += event.w6hActivation.reduce((s, v) => s + v, 0) / 7;
				countActivation++;
			}
			if (event.entropy != null) {
				sumEntropy += event.entropy;
				countEntropy++;
			}
		}

		const dayMeanActivation = countActivation > 0 ? sumActivation / countActivation : 0;
		const dayMeanEntropy = countEntropy > 0 ? sumEntropy / countEntropy : 0;

		// ── Second pass: compute deviations from day mean ────────────
		const xMin = -BOX_W / 2;
		const xMax = BOX_W / 2;
		const halfH = BOX_H / 2;
		const halfD = BOX_D / 2;

		// Points in group-local coordinates (Y=0, Z=0 = center line = day mean)
		const points: Vector3[] = [new Vector3(xMin, 0, 0)];
		const eventIds: string[] = [];

		let areaY = 0; // accumulated |y| × dx for activation area
		let areaZ = 0; // accumulated |z| × dx for entropy area
		let prevX = xMin;
		let prevY = 0;
		let prevZ = 0;

		for (const event of sorted) {
			const startH = getHourOfDay(event.startTime);
			const endH = getHourOfDay(event.endTime);
			const midH = (startH + endH) / 2;
			let x = xMin + (midH / 24) * BOX_W;

			// Enforce strictly monotonic X
			if (x <= prevX) x = prevX + 0.01;

			// Y: deviation from day mean activation, clamped to box
			let y = 0;
			if (event.w6hActivation) {
				const mean = event.w6hActivation.reduce((s, v) => s + v, 0) / 7;
				y = Math.max(-halfH, Math.min(halfH, (mean - dayMeanActivation) * Y_SCALE));
			}

			// Z: deviation from day mean entropy, clamped to box
			let z = 0;
			if (event.entropy != null) {
				z = Math.max(-halfD, Math.min(halfD, (event.entropy - dayMeanEntropy) * Z_SCALE));
			}

			// Trapezoidal area: average of |prev| and |curr| × width
			const dx = x - prevX;
			areaY += (Math.abs(prevY) + Math.abs(y)) / 2 * dx;
			areaZ += (Math.abs(prevZ) + Math.abs(z)) / 2 * dx;

			points.push(new Vector3(x, y, z));
			eventIds.push(event.id);
			prevX = x;
			prevY = y;
			prevZ = z;
		}

		// End anchor at day mean (center line)
		const dxEnd = xMax - prevX;
		areaY += (Math.abs(prevY) + 0) / 2 * dxEnd;
		areaZ += (Math.abs(prevZ) + 0) / 2 * dxEnd;
		points.push(new Vector3(xMax, 0, 0));

		// Normalize area by total width so it's a 0–1ish density, not dependent on BOX_W
		activationArea = areaY / BOX_W;
		entropyArea = areaZ / BOX_W;

		return { points, eventIds };
	}

	// ── Camera ──────────────────────────────────────────────────────────

	function fitCamera() {
		if (!camera || !containerEl) return;

		const w = containerEl.clientWidth;
		const h = containerEl.clientHeight;
		const aspect = w / h;

		const frontZ = BOX_D / 2;
		const vFov = (FOV * Math.PI) / 180;
		const hFov = 2 * Math.atan(Math.tan(vFov / 2) * aspect);

		// Distance from camera to the front edge so it fills horizontal FOV
		const distToFront = (BOX_W / 2) / Math.tan(hFov / 2);
		const dist = frontZ + distToFront;

		// Camera raised above midpoint — looks perfectly horizontal (no tilt)
		// so vertical edges stay perfectly vertical (no perspective convergence)
		const camY = BOX_H * 0.85;
		camera.position.set(0, camY, dist);
		camera.lookAt(new Vector3(0, camY, 0));
		camera.aspect = aspect;
		camera.updateProjectionMatrix();

		// Lens shift: offset projection downward to center the box in frame
		// without tilting the camera (architectural "shift lens" technique)
		const targetY = BOX_H * 0.38;
		const yShift = (targetY - camY) / (Math.tan(vFov / 2) * dist);
		camera.projectionMatrix.elements[9] = yShift;
	}

	function renderScene() {
		if (!renderer || !scene || !camera) return;
		renderer.render(scene, camera);
	}

	function rebuildScene() {
		if (!scene || !renderer || !camera) return;

		// Clear
		while (scene.children.length > 0) {
			const child = scene.children[0];
			scene.remove(child);
			if ('geometry' in child && 'dispose' in (child as Mesh).geometry) {
				(child as Mesh).geometry.dispose();
			}
			if ('material' in child) {
				const mat = (child as Mesh).material;
				if (Array.isArray(mat)) mat.forEach((m) => m.dispose());
				else if (mat && 'dispose' in mat) mat.dispose();
			}
		}

		const colors = getThemeColors();
		const xMin = -BOX_W / 2;
		const xMax = BOX_W / 2;

		const gridMat = new LineBasicMaterial({
			color: new Color(colors.foregroundSubtle),
			transparent: true,
			opacity: 0.12,
		});

		const axisMat = new LineBasicMaterial({
			color: new Color(colors.foreground),
			transparent: true,
			opacity: 0.5,
		});

		const centerLineMat = new LineBasicMaterial({
			color: new Color(colors.foreground),
			transparent: true,
			opacity: 0.35,
		});

		// Left face (YZ plane at x=xMin) — 8x8
		const leftFace = buildGridFace(
			new Vector3(xMin, 0, -BOX_D / 2),
			new Vector3(0, 1, 0),
			new Vector3(0, 0, 1),
			BOX_H, BOX_D,
			GRID, GRID,
		);
		scene.add(new LineSegments(leftFace, gridMat));

		// Right face (YZ plane at x=xMax) — 8x8
		const rightFace = buildGridFace(
			new Vector3(xMax, 0, -BOX_D / 2),
			new Vector3(0, 1, 0),
			new Vector3(0, 0, 1),
			BOX_H, BOX_D,
			GRID, GRID,
		);
		scene.add(new LineSegments(rightFace, gridMat));

		// Back face (XY plane at z=-BOX_D/2) — 24 columns (hours) x 8 rows
		const backFace = buildGridFace(
			new Vector3(xMin, 0, -BOX_D / 2),
			new Vector3(1, 0, 0),
			new Vector3(0, 1, 0),
			BOX_W, BOX_H,
			BACK_GRID_X, BACK_GRID_Y,
		);
		scene.add(new LineSegments(backFace, gridMat));

		// Bottom face (XZ plane at Y=0) — 24 columns (hours) x 8 rows (depth)
		const bottomFace = buildGridFace(
			new Vector3(xMin, 0, -BOX_D / 2),
			new Vector3(1, 0, 0),
			new Vector3(0, 0, 1),
			BOX_W, BOX_D,
			BACK_GRID_X, GRID,
		);
		scene.add(new LineSegments(bottomFace, gridMat));

		// Axis lines (more opaque)
		const axisGeo = buildAxisLines();
		scene.add(new LineSegments(axisGeo, axisMat));

		// Center line from left face to right face
		const centerLineGeo = buildCenterLine();
		scene.add(new LineSegments(centerLineGeo, centerLineMat));

		// Center dots on left and right faces
		const dotGeo = new SphereGeometry(0.035, 10, 10);
		const dotMat = new MeshBasicMaterial({
			color: new Color(colors.foreground),
			transparent: true,
			opacity: 0.7,
		});
		const leftDot = new Mesh(dotGeo, dotMat);
		leftDot.position.set(xMin, BOX_H / 2, 0);
		scene.add(leftDot);

		const rightDot = new Mesh(dotGeo.clone(), dotMat.clone());
		rightDot.position.set(xMax, BOX_H / 2, 0);
		scene.add(rightDot);

		// ── Data curve (in rotatable group) ─────────────────────────
		highlightStates.clear();
		activeHighlightId = null;
		hoverIntensity = 0;
		hoverTarget = 0;
		dataGroup = new Group();
		dataGroup.position.set(0, BOX_H / 2, 0); // pivot on center line
		dataGroup.rotation.x = currentRotation;
		scene.add(dataGroup);

		const { points: dataPoints, eventIds } = buildDataPoints();
		if (dataPoints.length >= 2) {
			// Main data polyline
			const curveGeo = new BufferGeometry().setFromPoints(dataPoints);
			const curveMat = new LineBasicMaterial({
				color: new Color(colors.primary),
				transparent: true,
				opacity: 0.5,
			});
			dataGroup.add(new Line(curveGeo, curveMat));

			// Dots at each event control point (skip centroid anchors)
			const evDotGeo = new SphereGeometry(0.04, 8, 8);
			const evDotMat = new MeshBasicMaterial({
				color: new Color(colors.primary),
				transparent: true,
				opacity: 0.9,
			});
			for (let i = 1; i < dataPoints.length - 1; i++) {
				const d = new Mesh(evDotGeo.clone(), evDotMat.clone());
				d.position.copy(dataPoints[i]);
				dataGroup.add(d);
			}

			// Per-event highlight overlays (prev → event → next) with animated glow
			const primaryColor = new Color(colors.primary);

			// Subtle glow offsets (cardinal directions only)
			const glowOffsets = [
				{ y: 0.025, z: 0 },
				{ y: -0.025, z: 0 },
				{ y: 0, z: 0.025 },
				{ y: 0, z: -0.025 },
			];

			for (let i = 0; i < eventIds.length; i++) {
				const ptIdx = i + 1;
				const prev = dataPoints[ptIdx - 1];
				const curr = dataPoints[ptIdx];
				const next = dataPoints[ptIdx + 1];
				const hlGroup = new Group();
				hlGroup.visible = false;

				// Core solid line (opacity driven by animation)
				const coreGeo = new BufferGeometry().setFromPoints([prev, curr, next]);
				const coreMat = new LineBasicMaterial({
					color: primaryColor,
					transparent: true,
					opacity: 0,
				});
				hlGroup.add(new Line(coreGeo, coreMat));

				// Glow lines at offsets (additive blending)
				const glowMats: LineBasicMaterial[] = [];
				for (const offset of glowOffsets) {
					const pts = [prev, curr, next].map(
						(p) => new Vector3(p.x, p.y + offset.y, p.z + offset.z),
					);
					const glowGeo = new BufferGeometry().setFromPoints(pts);
					const glowMat = new LineBasicMaterial({
						color: primaryColor,
						transparent: true,
						opacity: 0,
						blending: AdditiveBlending,
						depthWrite: false,
					});
					glowMats.push(glowMat);
					hlGroup.add(new Line(glowGeo, glowMat));
				}

				// Dot at event point (scale animated from 0)
				const hlDotGeo = new SphereGeometry(0.07, 10, 10);
				const hlDotMat = new MeshBasicMaterial({
					color: primaryColor,
					transparent: true,
					opacity: 0.9,
				});
				const hlDot = new Mesh(hlDotGeo, hlDotMat);
				hlDot.position.copy(curr);
				hlDot.scale.setScalar(0.001);
				hlGroup.add(hlDot);

				// Halo around dot (additive, scale animated from 0)
				const haloDotGeo = new SphereGeometry(0.13, 10, 10);
				const haloMat = new MeshBasicMaterial({
					color: primaryColor,
					transparent: true,
					opacity: 0,
					blending: AdditiveBlending,
					depthWrite: false,
				});
				const haloDot = new Mesh(haloDotGeo, haloMat);
				haloDot.position.copy(curr);
				haloDot.scale.setScalar(0.001);
				hlGroup.add(haloDot);

				dataGroup.add(hlGroup);
				highlightStates.set(eventIds[i], {
					group: hlGroup,
					coreMat,
					glowMats,
					dot: hlDot,
					halo: haloDot,
					haloMat: haloMat,
				});
			}
		} else if (dataPoints.length === 1) {
			const evDotGeo = new SphereGeometry(0.04, 8, 8);
			const evDotMat = new MeshBasicMaterial({
				color: new Color(colors.primary),
				transparent: true,
				opacity: 0.9,
			});
			const d = new Mesh(evDotGeo, evDotMat);
			d.position.copy(dataPoints[0]);
			dataGroup.add(d);
		}

		fitCamera();
		renderScene();
	}

	// React to hoveredEventId changes — set animation target
	$effect(() => {
		const id = hoveredEventId;
		if (id) {
			// Switch to new event: snap-hide old, start fading in new
			if (activeHighlightId && activeHighlightId !== id) {
				const prev = highlightStates.get(activeHighlightId);
				if (prev) {
					prev.group.visible = false;
					applyHoverIntensity(prev, 0);
				}
			}
			activeHighlightId = id;
			const state = highlightStates.get(id);
			if (state) {
				state.group.visible = true;
				hoverIntensity = 0;
				applyHoverIntensity(state, 0);
				hoverTarget = 1;
			}
		} else if (activeHighlightId) {
			// Fade out current highlight
			hoverTarget = 0;
		}
	});

	// ── Animation loop (ease-back + hover fade) ────────────────────────

	let lastFrameTime = 0;

	/** Ease-in-out cubic: smooth deceleration at both ends */
	function easeInOutCubic(t: number): number {
		return t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;
	}

	function animate(time: number) {
		animFrameId = requestAnimationFrame(animate);

		const dt = Math.min((time - lastFrameTime) / 1000, 0.1);
		lastFrameTime = time;

		let needsRender = false;

		// Ease-back rotation
		if (dataGroup && !isDragging && isEasingBack) {
			easeBackTime += dt;
			const t = Math.min(easeBackTime / EASE_BACK_DURATION, 1);
			currentRotation = easeBackStart * (1 - easeInOutCubic(t));
			dataGroup.rotation.x = currentRotation;
			needsRender = true;
			if (t >= 1) {
				isEasingBack = false;
				currentRotation = 0;
			}
		}

		// Hover intensity fade
		if (activeHighlightId) {
			const state = highlightStates.get(activeHighlightId);
			if (state) {
				const diff = hoverTarget - hoverIntensity;
				if (Math.abs(diff) > 0.001) {
					const step = HOVER_SPEED * dt;
					hoverIntensity = diff > 0
						? Math.min(hoverTarget, hoverIntensity + step)
						: Math.max(hoverTarget, hoverIntensity - step);
					applyHoverIntensity(state, hoverIntensity);
					needsRender = true;
				}
				// Fully faded out — hide group and clear
				if (hoverIntensity <= 0.001 && hoverTarget === 0) {
					state.group.visible = false;
					activeHighlightId = null;
					hoverIntensity = 0;
					needsRender = true;
				}
			}
		}

		if (needsRender) renderScene();
	}

	function startEaseBack() {
		if (Math.abs(currentRotation) < 0.001) {
			currentRotation = 0;
			if (dataGroup) dataGroup.rotation.x = 0;
			renderScene();
			return;
		}
		easeBackStart = currentRotation;
		easeBackTime = 0;
		isEasingBack = true;
	}

	// ── Mouse interaction ──────────────────────────────────────────────

	function handleMouseDown(e: MouseEvent) {
		isDragging = true;
		isEasingBack = false;
		dragStartY = e.clientY;
		dragStartRotation = currentRotation;
	}

	function handleMouseMove(e: MouseEvent) {
		if (!isDragging) return;
		const deltaY = e.clientY - dragStartY;
		// Map pixel drag to rotation (200px = full PI rotation)
		currentRotation = dragStartRotation + (deltaY / 200) * Math.PI;
		if (dataGroup) {
			dataGroup.rotation.x = currentRotation;
			renderScene();
		}
	}

	function handleMouseUp() {
		if (!isDragging) return;
		isDragging = false;
		startEaseBack();
	}

	function handleMouseLeave() {
		if (isDragging) {
			isDragging = false;
			startEaseBack();
		}
	}

	onMount(() => {
		if (!canvasEl || !containerEl) return;

		renderer = new WebGLRenderer({
			canvas: canvasEl,
			antialias: true,
			alpha: true,
		});
		renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
		renderer.setClearColor(0x000000, 0);

		const w = containerEl.clientWidth;
		const h = containerEl.clientHeight;
		renderer.setSize(w, h);

		scene = new Scene();
		camera = new PerspectiveCamera(FOV, w / h, 0.1, 500);

		resizeObserver = new ResizeObserver(() => {
			if (!containerEl || !renderer || !camera) return;
			const rw = containerEl.clientWidth;
			const rh = containerEl.clientHeight;
			renderer.setSize(rw, rh);
			camera.aspect = rw / rh;
			camera.updateProjectionMatrix();
			fitCamera();
			renderScene();
		});
		resizeObserver.observe(containerEl);

		themeObserver = new MutationObserver(() => {
			rebuildScene();
		});
		themeObserver.observe(document.documentElement, {
			attributes: true,
			attributeFilter: ['class', 'data-theme'],
		});

		rebuildScene();
		animFrameId = requestAnimationFrame(animate);
	});

	onDestroy(() => {
		if (animFrameId != null) cancelAnimationFrame(animFrameId);
		resizeObserver?.disconnect();
		themeObserver?.disconnect();
		renderer?.dispose();
		renderer = null;
		scene = null;
		camera = null;
		dataGroup = null;
	});
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="ribbon-chart"
	class:dragging={isDragging}
	bind:this={containerEl}
	onmousedown={handleMouseDown}
	onmousemove={handleMouseMove}
	onmouseup={handleMouseUp}
	onmouseleave={handleMouseLeave}
>
	<canvas bind:this={canvasEl}></canvas>
	<div class="legend">
		<div class="legend-axes">
			<span class="legend-axis">&#8597; Activation</span>
			<span class="legend-axis">&#8599; Semantic Entropy</span>
		</div>
		<div class="legend-metrics">
			<span class="legend-metric">Contrast {(activationArea + entropyArea).toFixed(2)}</span>
		</div>
	</div>
</div>

<style>
	.ribbon-chart {
		width: 100%;
		height: 300px;
		margin-bottom: 0;
		position: relative;
	}

	.ribbon-chart canvas {
		width: 100%;
		height: 100%;
		display: block;
		cursor: grab;
	}

	.ribbon-chart.dragging canvas {
		cursor: grabbing;
	}

	.legend {
		position: absolute;
		bottom: 0.75rem;
		right: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		padding: 0.375rem 0.5rem;
		background: color-mix(in srgb, var(--color-surface) 80%, transparent);
		border-radius: 4px;
		pointer-events: none;
		font-family: var(--font-mono, monospace);
		font-size: 0.625rem;
		color: var(--color-foreground-muted);
		line-height: 1.4;
	}

	.legend-axes {
		display: flex;
		gap: 0.75rem;
	}

	.legend-axis {
		opacity: 0.7;
	}

	.legend-metrics {
		display: flex;
		gap: 0.75rem;
	}

	.legend-metric {
		opacity: 0.9;
	}
</style>
