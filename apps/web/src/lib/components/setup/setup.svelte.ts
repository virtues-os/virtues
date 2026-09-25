/**
 * Setup — the one flow, and the order of its steps.
 *
 * ONE PROCESS, ONE WORD (2026-09-24, agents/plan/setup-plan.md). Setup runs
 * full screen at `/setup`: nine steps with a dot each — Welcome (the cold
 * open, and light or dark), the founder's letter, then the seven that do
 * something — then the ∴ and the app. Everything up to Names is required;
 * the last three can each be skipped, and "Finish later" opens the app with
 * the rest waiting on the rail's Setup tile. It replaced the Getting started room
 * (a stepper in the app's pane, 2026-09-23), which replaced a seeded chat.
 *
 * NOTHING HERE IS STORED PROGRESS. Every status is read off real rows — the
 * server's derived getting-started state, the profile, the paired devices,
 * the streams arriving — so a step that is true elsewhere (a phone paired
 * from Settings) is already ticked, and reopening the app lands on the first
 * step still to do. The one stored fact is a skip — and, on this device
 * only, how far through Welcome and the letter someone has read, which no
 * row can say (see `introStage`).
 *
 * Server and Wi-Fi are done by construction in the web app: nothing reaches
 * `/setup` without a paired device on a connected server. They move into
 * this flow when it runs before pairing (the plan's slice 2).
 */
import {
	getProfile,
	getAssistantProfile,
	getStreamHealth,
	listCredentials,
	type Profile,
	type StreamHealth,
	type Credential,
	type GettingStartedStepId,
} from '$lib/api/client';
import { gettingStarted } from '$lib/stores/gettingStarted.svelte';

export type SetupStepId =
	| 'welcome'
	| 'letter'
	| 'server'
	| 'wifi'
	| 'subscription'
	| 'names'
	| 'connections'
	| 'timeline'
	| 'interview';

export type StepStatus = 'done' | 'open' | 'skipped';

/** A part of a step, ticked on its own ("App ✓ · Permissions"). */
export interface StepPart {
	label: string;
	done: boolean;
}

export interface SetupStep {
	id: SetupStepId;
	/** The dot's label, one word or two. */
	label: string;
	status: StepStatus;
	/** Skippable steps show Skip and Finish later; required ones neither. */
	optional: boolean;
	/** Parts shown under the row in the rail's panel. */
	parts?: { group?: string; items: StepPart[] }[];
	/** Forward is strict, backward is free: a step past the first one still
	 *  open cannot be entered, because every step after it assumes it. */
	reachable: boolean;
}

export const ORDER: SetupStepId[] = [
	'welcome',
	'letter',
	'server',
	'wifi',
	'subscription',
	'names',
	'connections',
	'timeline',
	'interview',
];

const LABELS: Record<SetupStepId, string> = {
	welcome: 'Welcome',
	letter: 'Letter',
	server: 'Server',
	wifi: 'Wi-Fi',
	subscription: 'Subscription',
	names: 'Names',
	connections: 'Connections',
	timeline: 'Timeline',
	interview: 'Interview',
};

const OPTIONAL = new Set<SetupStepId>(['connections', 'timeline', 'interview']);

/** The server's step each flow step reads its status from, if any. */
export const SERVER: Record<SetupStepId, GettingStartedStepId | null> = {
	welcome: null,
	letter: null,
	server: null,
	wifi: null,
	subscription: 'connect_ai',
	names: 'introductions',
	connections: 'connect_world',
	timeline: 'timeline',
	interview: 'interview',
};

export function isSetupStep(s: string | null | undefined): s is SetupStepId {
	return !!s && (ORDER as string[]).includes(s);
}

export function labelOf(id: SetupStepId): string {
	return LABELS[id];
}

/** A button that leads INTO a step says what the person does there. */
const INTO: Record<SetupStepId, string> = {
	welcome: 'Begin',
	letter: 'Read the letter',
	server: 'Find your server',
	wifi: 'Connect to Wi-Fi',
	subscription: 'Choose how your assistant thinks',
	names: 'Name your assistant',
	connections: 'Connect your devices',
	timeline: 'Draw your timeline',
	interview: 'Start the interview',
};

/** The label for a button that moves on to `id`, or opens the app. */
export function intoLabel(id: SetupStepId | null): string {
	return id ? INTO[id] : 'Open Virtues';
}

/** A stream counts as arriving once anything has landed this week. */
function arriving(streams: StreamHealth[], name: string): boolean {
	const s = streams.find((x) => x.name === name);
	return !!s && (s.status === 'live' || s.status === 'stalled' || s.count_7d > 0);
}

function paired(creds: Credential[], provider: string): boolean {
	return creds.some((c) => c.provider === provider && c.is_active);
}

/** How far through the reading someone is on this device: 0 nothing, 1 past
 *  Welcome, 2 past the letter. A per-device convenience in localStorage —
 *  losing it only replays the cold open, and a server where anything later
 *  is done counts both as read regardless. */
const INTRO_KEY = 'virtues-setup-intro';
function readIntro(): number {
	try {
		return Number(localStorage.getItem(INTRO_KEY) ?? 0) || 0;
	} catch {
		return 0;
	}
}

class SetupStore {
	profile = $state<Profile | null>(null);
	assistantName = $state('Ari');
	streams = $state<StreamHealth[]>([]);
	credentials = $state<Credential[]>([]);
	loaded = $state(false);
	introStage = $state(typeof window === 'undefined' ? 0 : readIntro());
	private inflight: Promise<void> | null = null;

	/** Record that Welcome (1) or the letter (2) has been passed. */
	passIntro(stage: 1 | 2): void {
		if (this.introStage >= stage) return;
		this.introStage = stage;
		try {
			localStorage.setItem(INTRO_KEY, String(stage));
		} catch {
			/* a replayed cold open is the whole cost */
		}
	}

	refresh(): Promise<void> {
		if (this.inflight) return this.inflight;
		this.inflight = (async () => {
			// Each read stands alone: a failed stream read must not hide the
			// profile, and none of them may hold the flow shut.
			const [profile, assistant, streams, creds] = await Promise.allSettled([
				getProfile(),
				getAssistantProfile<{ assistant_name?: string | null }>(),
				getStreamHealth(),
				listCredentials(),
				gettingStarted.refresh(),
			]);
			if (profile.status === 'fulfilled') this.profile = profile.value;
			if (assistant.status === 'fulfilled') this.assistantName = assistant.value.assistant_name || 'Ari';
			if (streams.status === 'fulfilled') this.streams = streams.value;
			if (creds.status === 'fulfilled') this.credentials = creds.value;
			this.loaded = true;
			this.inflight = null;
		})();
		return this.inflight;
	}

	/** The computer: the app, then the permissions that make it useful. */
	get computer(): StepPart[] {
		return [
			{ label: 'App', done: paired(this.credentials, 'mac') },
			{
				label: 'Permissions',
				done:
					arriving(this.streams, 'communication_message') ||
					arriving(this.streams, 'activity_web_browsing') ||
					arriving(this.streams, 'activity_app_session'),
			},
		];
	}

	/** The phone: one badge per stream it carries. */
	get phone(): StepPart[] {
		return [
			{ label: 'Location', done: arriving(this.streams, 'location_point') },
			{ label: 'Audio', done: arriving(this.streams, 'communication_transcription') },
			{
				label: 'Health',
				done:
					arriving(this.streams, 'health_steps') ||
					arriving(this.streams, 'health_heart_rate') ||
					arriving(this.streams, 'health_sleep'),
			},
		];
	}

	/**
	 * What a device has already sent, as one line: the payoff for turning it
	 * on, read off the same stream counts that light the badges. Null until
	 * something has arrived. Counts are this week's, and each noun names
	 * exactly what the stream counts (a location update is not a place).
	 */
	arrived(device: 'computer' | 'phone'): string | null {
		const n = (name: string) => this.streams.find((s) => s.name === name)?.count_7d ?? 0;
		const parts: [number, string, string][] =
			device === 'computer'
				? [
						[n('communication_message'), 'message', 'messages'],
						[n('activity_web_browsing'), 'page read', 'pages read'],
						[n('activity_app_session'), 'app session', 'app sessions'],
					]
				: [
						[n('location_point'), 'location update', 'location updates'],
						[n('communication_transcription'), 'recording', 'recordings'],
						[n('health_steps') + n('health_heart_rate') + n('health_sleep'), 'Health reading', 'Health readings'],
					];
		const said = parts
			.filter(([c]) => c > 0)
			.map(([c, one, many]) => `${c.toLocaleString()} ${c === 1 ? one : many}`);
		return said.length ? `Arrived this week: ${said.join(', ')}.` : null;
	}

	get phonePaired(): boolean {
		return paired(this.credentials, 'ios');
	}

	status(id: SetupStepId): StepStatus {
		if (id === 'welcome' || id === 'letter') {
			const need = id === 'welcome' ? 1 : 2;
			return this.introStage >= need || !this.fresh ? 'done' : 'open';
		}
		const key = SERVER[id];
		if (!key) return 'done';
		const own = gettingStarted.step(key)?.status;
		if (own) return own;
		// A server older than the timeline step: its interview status is the
		// nearest truth (it once counted drawn chapters too).
		if (id === 'timeline') return gettingStarted.step('interview')?.status ?? 'open';
		// An older server with no getting-started state at all is treated as
		// set up, the way the store treats it — never strand someone on it.
		return gettingStarted.unsupported ? 'done' : 'open';
	}

	get steps(): SetupStep[] {
		let open = true;
		return ORDER.map((id) => {
			const status = this.status(id);
			const reachable = open;
			if (status === 'open') open = false;
			const step: SetupStep = { id, label: LABELS[id], status, optional: OPTIONAL.has(id), reachable };
			if (id === 'connections') {
				step.parts = [
					{ group: 'Computer', items: this.computer },
					{ group: 'iPhone', items: [{ label: 'Paired', done: this.phonePaired }, ...this.phone] },
				];
			}
			return step;
		});
	}

	/** Where setup picks up: the first step still open, else the first set
	 *  aside, else nothing — setup is finished. */
	get resumeAt(): SetupStepId | null {
		const steps = this.steps;
		return (
			steps.find((s) => s.status === 'open')?.id ?? steps.find((s) => s.status === 'skipped')?.id ?? null
		);
	}

	/** Nothing the person does in Setup has happened yet: the flow opens
	 *  with Hello and the letter. Anyone further along resumes at their step.
	 *
	 *  The subscription is NOT part of this. It can be settled before anyone
	 *  reaches /setup — the pairing screen's "I already have an account"
	 *  links the server before pairing, and a dev server marks it done — and
	 *  keying on it sent those people past Hello straight into Names. */
	get fresh(): boolean {
		return (['names', 'connections', 'timeline', 'interview'] as SetupStepId[]).every(
			(id) => this.status(id) === 'open',
		);
	}

	/** The required steps are behind them, so the app may open. */
	get requiredDone(): boolean {
		return this.steps.every((s) => s.optional || s.status !== 'open');
	}

	get doneCount(): number {
		return this.steps.filter((s) => s.status === 'done').length;
	}

	/** Every step done: the rail's Setup tile goes. A skipped step keeps it. */
	get complete(): boolean {
		return this.steps.every((s) => s.status === 'done');
	}

	/** Where moving on from `after` goes: the next step still to do, the
	 *  same rule the flow's advance follows. Null opens the app. */
	upNext(after: SetupStepId): SetupStepId | null {
		const steps = this.steps;
		const i = steps.findIndex((s) => s.id === after);
		return steps.slice(i + 1).find((s) => s.status !== 'done')?.id ?? null;
	}

	next(after: SetupStepId): SetupStepId | null {
		const i = ORDER.indexOf(after);
		return i >= 0 && i < ORDER.length - 1 ? ORDER[i + 1] : null;
	}
}

export const setup = new SetupStore();
