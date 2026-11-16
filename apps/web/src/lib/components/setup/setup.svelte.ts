/**
 * Setup — the one flow, and the order of its steps.
 *
 * ONE PROCESS, ONE WORD (agents/plan/onboarding-diy-plan.md). Setup runs
 * full screen at `/setup`: Welcome (the cold open, and light or dark),
 * Server (finding and pairing it), Names, this device's Connections,
 * Subscription, then Arrival and the app. Connections can be skipped; its
 * "Finish later" opens the app only once nothing required follows it.
 * Everything else is required. The founder's letter waits at Arrival.
 * Chapters and the interview are not part of setup.
 *
 * NOTHING HERE IS STORED PROGRESS. Every status is read off real rows — the
 * server's derived getting-started state, the profile, the paired devices,
 * the streams arriving — so a step that is true elsewhere (a phone paired
 * from Settings) is already ticked, and reopening the app lands on the first
 * step still to do. The one stored fact is a skip — and, on this device
 * only, how far through Welcome and the letter someone has read, which no
 * row can say (see `introStage`).
 *
 * Server is the first half, before this device has a server
 * (`prepair.svelte.ts`). It runs here from the app's own copy; anywhere
 * Setup is reached through a server it is done by design, since nothing gets
 * there without a paired device on a connected server.
 */
import {
	getProfile,
	getAssistantProfile,
	getStreamHealth,
	listDevices,
	type Profile,
	type StreamHealth,
	type GettingStartedStepId,
} from '$lib/api/client';
import type { Device, DevicesResponse } from '$lib/devices/shared';
import { gettingStarted } from '$lib/stores/gettingStarted.svelte';
import { prePair } from './prepair.svelte';

export type SetupStepId =
	| 'welcome'
	| 'letter'
	| 'server'
	| 'subscription'
	| 'names'
	| 'connections';

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

/** The stage's steps, in order. The letter is not one of them: the pair
 *  code lasts about 15 minutes, so nothing stands between the person and
 *  it. The letter waits at the close, as "A note from us", and stays
 *  readable in the app (IN_APP) and at /founders-letter. */
export const ORDER: SetupStepId[] = [
	'welcome',
	'server',
	// The names straight after the server: ten seconds, and from then on
	// the assistant has a name and the person has been asked theirs.
	'names',
	// This device before the subscription: pairing and collecting need no
	// account, and Messages and Safari history are the slowest sources to
	// arrive, so they start while the person finishes the rest.
	'connections',
	'subscription',
];
// TODO(2026-11-15): steps/StepTimeline.svelte and steps/StepInterview.svelte
// left setup and nothing imports them. Delete them, with ChapterExample and
// nextChapter.ts, unless the home chapters get (onboarding-diy-plan.md,
// decision 4) reuses them.

const LABELS: Record<SetupStepId, string> = {
	welcome: 'Welcome',
	letter: 'Letter',
	server: 'Server',
	subscription: 'Subscription',
	names: 'Names',
	connections: 'Connections',
};

const OPTIONAL = new Set<SetupStepId>(['connections']);

/** The server's step each flow step reads its status from, if any. */
export const SERVER: Record<SetupStepId, GettingStartedStepId | null> = {
	welcome: null,
	letter: null,
	server: null,
	subscription: 'connect_ai',
	names: 'introductions',
	connections: 'connect_world',
};

export function isSetupStep(s: string | null | undefined): s is SetupStepId {
	return !!s && (ORDER as string[]).includes(s);
}

export function labelOf(id: SetupStepId): string {
	return LABELS[id];
}

/** The steps Setup offers inside the app (SetupView, the rail's panel), in
 *  order. The first half is finished for anyone in the app, and Welcome's
 *  opening only plays on the stage. The letter is here to read again. */
export const IN_APP: SetupStepId[] = ['letter', 'names', 'connections', 'subscription'];

/** A button that leads INTO a step says what the person does there. */
const INTO: Record<SetupStepId, string> = {
	welcome: 'Begin',
	letter: 'Read the letter',
	server: 'Find your server',
	subscription: 'Choose how your assistant thinks',
	names: 'Name your assistant',
	connections: 'Add your devices',
};

/** The label for a button that moves on to `id`, or opens the app. */
export function intoLabel(id: SetupStepId | null): string {
	if (id === 'subscription') return `Choose how ${setup.assistantName} thinks`;
	return id ? INTO[id] : 'Open Virtues';
}

/** A stream counts as arriving once anything has landed this week. */
function arriving(streams: StreamHealth[], name: string): boolean {
	const s = streams.find((x) => x.name === name);
	return !!s && (s.status === 'live' || s.status === 'stalled' || s.count_7d > 0);
}

/**
 * The paired device that collects for a source, from the server's device
 * list. Pairing records a device and no credential, so asking the
 * credentials (as this once did) never found one. For the computer it is
 * the collector's record, the one that carries `installed_by` or reported
 * permissions: the desktop app's own pairing also names `mac` and collects
 * nothing.
 */
function collectorFor(devices: Device[], source: 'mac' | 'ios'): Device | null {
	return (
		devices.find((d) =>
			source === 'ios'
				? d.kind === 'mobile_app'
				: d.source_id === 'mac' && (d.installed_by !== null || d.permissions !== null),
		) ?? null
	);
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
	devices = $state<Device[]>([]);
	loaded = $state(false);
	introStage = $state(typeof window === 'undefined' ? 0 : readIntro());
	/** Keystrokes in a step that is talking to the assistant (Names). The
	 *  mark at the top ripples on each: it is the assistant listening. */
	heard = $state(0);

	hear(): void {
		this.heard++;
	}
	private inflight: Promise<void> | null = null;

	/** Record that Welcome (1) or the letter (2) has been passed. The stage
	 *  passes only Welcome; the letter is read in the app. */
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
		// No server yet: there is nothing to read, and every step after the
		// first half is simply still to do.
		if (prePair.active) {
			this.loaded = true;
			return Promise.resolve();
		}
		if (this.inflight) return this.inflight;
		this.inflight = (async () => {
			// Each read stands alone: a failed stream read must not hide the
			// profile, and none of them may hold the flow shut.
			const [profile, assistant, streams, devices] = await Promise.allSettled([
				getProfile(),
				getAssistantProfile<{ assistant_name?: string | null }>(),
				getStreamHealth(),
				listDevices<DevicesResponse>(),
				gettingStarted.refresh(),
			]);
			if (profile.status === 'fulfilled') this.profile = profile.value;
			if (assistant.status === 'fulfilled') this.assistantName = assistant.value.assistant_name || 'Ari';
			if (streams.status === 'fulfilled') this.streams = streams.value;
			if (devices.status === 'fulfilled') this.devices = devices.value.devices ?? [];
			this.loaded = true;
			this.inflight = null;
		})();
		return this.inflight;
	}

	/** The computer: the app, then the permissions that make it useful. */
	get computer(): StepPart[] {
		return [
			{ label: 'Paired', done: !!this.mac },
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
			{ label: 'Microphone', done: arriving(this.streams, 'communication_transcription') },
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
	 * What a device has started sending, as one line: the payoff for turning
	 * it on, read off the same streams that light the badges. Null until
	 * something has arrived. It names what is arriving and never how much:
	 * setup shows no counts, since someone in recovery sees every page.
	 */
	arrived(device: 'computer' | 'phone'): string | null {
		const list = this.arriving(device);
		return list ? `Your server is receiving ${list}.` : null;
	}

	/** What a device has started sending, as a list of nouns ("your messages
	 *  and the pages you read"), or null. Never how much. */
	arriving(device: 'computer' | 'phone'): string | null {
		const has = (...names: string[]) => names.some((name) => arriving(this.streams, name));
		const parts: [boolean, string][] =
			device === 'computer'
				? [
						[has('communication_message'), 'your messages'],
						[has('activity_web_browsing'), 'the pages you read'],
						[has('activity_app_session'), 'the apps you use'],
					]
				: [
						[has('location_point'), 'where you go'],
						[has('communication_transcription'), 'your recordings'],
						[has('health_steps', 'health_heart_rate', 'health_sleep'), 'your Health data'],
					];
		const said = parts.filter(([on]) => on).map(([, noun]) => noun);
		if (!said.length) return null;
		return said.length === 1 ? said[0] : `${said.slice(0, -1).join(', ')} and ${said[said.length - 1]}`;
	}

	/** The computer's collector and the phone, once paired. */
	get mac(): Device | null {
		return collectorFor(this.devices, 'mac');
	}
	get iphone(): Device | null {
		return collectorFor(this.devices, 'ios');
	}
	get phonePaired(): boolean {
		return !!this.iphone;
	}

	status(id: SetupStepId): StepStatus {
		if (id === 'welcome' || id === 'letter') {
			const need = id === 'welcome' ? 1 : 2;
			return this.introStage >= need || !this.fresh ? 'done' : 'open';
		}
		// The first half: settled here, on this device, before any server.
		if (id === 'server') return prePair.active ? 'open' : 'done';
		const key = SERVER[id];
		if (!key) return 'done';
		const own = gettingStarted.step(key)?.status;
		// A required step can't be set aside. The server still keeps skips a
		// retired command once wrote against the subscription, and taking one
		// at its word walked people past a step Setup never offers to skip.
		if (own === 'skipped' && !OPTIONAL.has(id)) return 'open';
		if (own) return own;
		// An older server with no getting-started state at all is treated as
		// set up, the way the store treats it — never strand someone on it.
		return gettingStarted.unsupported ? 'done' : 'open';
	}

	get steps(): SetupStep[] {
		return this.stepsIn(ORDER);
	}

	/** `ids` as steps, in that order: the stage's (ORDER) or the app's
	 *  (IN_APP, which also lists the letter to reread). A step is reachable
	 *  while every one before it in `ids` is settled. */
	stepsIn(ids: SetupStepId[]): SetupStep[] {
		let open = true;
		return ids.map((id) => {
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

	/** In the app: the first step there still open, else the first set
	 *  aside, else nothing. The letter is only ever reread, never owed. */
	get resumeInApp(): SetupStepId | null {
		const steps = this.stepsIn(IN_APP).filter((s) => s.id !== 'letter');
		return (
			steps.find((s) => s.status === 'open')?.id ?? steps.find((s) => s.status === 'skipped')?.id ?? null
		);
	}

	/** In the app, a step opens once the required steps before it there are
	 *  done. Not the stage's strict order: coming back, the person is
	 *  choosing a step, not walking them; and the letter and Welcome, which
	 *  count as read only on the device that read them, never lock anything. */
	opensInApp(id: SetupStepId): boolean {
		const steps = this.stepsIn(IN_APP).filter((s) => s.id !== 'letter');
		const at = steps.findIndex((s) => s.id === id);
		return at < 0 || steps.slice(0, at).every((s) => s.optional || s.status === 'done');
	}

	/** Nothing the person does in Setup has happened yet: the flow opens
	 *  with Hello. Anyone further along resumes at their step.
	 *
	 *  The subscription is NOT part of this. It can be settled before anyone
	 *  reaches /setup — the pairing screen's "I already have an account"
	 *  links the server before pairing, and a dev server marks it done — and
	 *  keying on it sent those people past Hello straight into Names. */
	get fresh(): boolean {
		return (['connections', 'names'] as SetupStepId[]).every(
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
