/**
 * Startup timing: where a cold start spends its time, from navigation to a
 * composer you can type in. Each phase is a `performance.mark` named
 * `startup:<phase>`; `startupReport()` reads them back with the scripts'
 * download times and the requests made before the shell rendered.
 *
 * Nothing here is shown in the app or sent anywhere. Read it in the
 * webview's console with `__startup()`, or set `localStorage.startupLog = '1'`
 * to have the report logged once the composer is ready. A mark that comes
 * after the first startup (a later layout mount, a second composer) is not
 * recorded, so the report describes the launch only.
 */

export type StartupPhase =
	| 'script' // the app's code has downloaded, parsed and started running
	| 'load-start' // the shell's data load began
	| 'load-end' // the shell's data load finished
	| 'shell-mounted' // the shell's layout mounted
	| 'shell-ready' // the shell drew its panes
	| 'painted' // the frame after the shell drew its panes
	| 'composer-ready'; // the composer's editor exists

const PREFIX = 'startup:';
const done = new Set<StartupPhase>();

function supported(): boolean {
	return typeof performance !== 'undefined' && typeof performance.mark === 'function';
}

/** Record a phase of the launch, once. */
export function markStartup(phase: StartupPhase): void {
	if (!supported() || done.has(phase)) return;
	done.add(phase);
	performance.mark(PREFIX + phase);
	if (phase === 'shell-ready') {
		// Two frames: the first runs before the browser paints the shell, the
		// second after it has.
		requestAnimationFrame(() => requestAnimationFrame(() => markStartup('painted')));
	}
	if (phase === 'composer-ready' && wantsLog()) {
		// After the frame, so the report includes the paint.
		setTimeout(() => console.info('[startup]', JSON.stringify(startupReport(), null, 2)), 0);
	}
}

function wantsLog(): boolean {
	try {
		return localStorage.getItem('startupLog') === '1';
	} catch {
		return false;
	}
}

const ms = (n: number) => Math.round(n);

export type StartupReport = {
	/** Milliseconds from navigation start to each phase. */
	phases: Partial<Record<StartupPhase | 'first-contentful-paint' | 'html', number>>;
	/** The app's JavaScript: how many files, their size, and when the last one arrived. */
	scripts: { files: number; kb: number; lastEnd: number };
	/** Requests to the server that finished before the shell drew. */
	requestsBeforeShell: { path: string; start: number; end: number }[];
	/** Tasks over 50 ms on the main thread before the composer was ready, where the browser reports them. */
	longTasks: { start: number; duration: number }[];
};

let longTasks: { start: number; duration: number }[] = [];
if (typeof PerformanceObserver !== 'undefined') {
	try {
		const observer = new PerformanceObserver((list) => {
			for (const e of list.getEntries()) longTasks.push({ start: ms(e.startTime), duration: ms(e.duration) });
		});
		observer.observe({ type: 'longtask', buffered: true });
	} catch {
		// WebKit has no long-task timing; the report leaves them out.
		longTasks = [];
	}
}

/** The launch's timings, in milliseconds from navigation start. */
export function startupReport(): StartupReport {
	const phases: StartupReport['phases'] = {};
	for (const m of performance.getEntriesByType('mark')) {
		if (m.name.startsWith(PREFIX)) phases[m.name.slice(PREFIX.length) as StartupPhase] = ms(m.startTime);
	}
	const fcp = performance.getEntriesByType('paint').find((p) => p.name === 'first-contentful-paint');
	if (fcp) phases['first-contentful-paint'] = ms(fcp.startTime);
	const nav = performance.getEntriesByType('navigation')[0] as PerformanceNavigationTiming | undefined;
	if (nav) phases.html = ms(nav.responseEnd);

	const resources = performance.getEntriesByType('resource') as PerformanceResourceTiming[];
	const js = resources.filter((r) => r.name.includes('/_app/') && r.name.endsWith('.js'));
	const shellReady = phases['shell-ready'] ?? Infinity;
	const composerReady = phases['composer-ready'] ?? Infinity;
	return {
		phases,
		scripts: {
			files: js.length,
			kb: ms(js.reduce((n, r) => n + (r.encodedBodySize || 0), 0) / 1024),
			lastEnd: ms(Math.max(0, ...js.map((r) => r.responseEnd))),
		},
		requestsBeforeShell: resources
			.filter((r) => r.name.includes('/api/') && r.responseEnd <= shellReady)
			.map((r) => ({ path: new URL(r.name).pathname, start: ms(r.startTime), end: ms(r.responseEnd) })),
		longTasks: longTasks.filter((t) => t.start <= composerReady),
	};
}

if (typeof window !== 'undefined') {
	(window as unknown as { __startup?: () => StartupReport }).__startup = startupReport;
}
