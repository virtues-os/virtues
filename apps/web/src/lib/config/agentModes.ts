/**
 * Agent Mode Configuration
 *
 * The modes the composer cycles through with Shift+Tab. The id goes to the
 * server as `agentMode`, which picks the turn's tools and prompt
 * (`ChatMode::tools`, `agent::prompt`).
 *
 * `sudo` is the owner's admin mode: a shell on the server with passwordless
 * sudo. Commands run without asking; one that deletes data waits for Allow
 * on the exact command (`tools::sudo_gate`). It lasts for one chat and starts off every time a chat
 * opens.
 *
 * `local` runs a small model on the server's NPU and nowhere else. It is
 * offered only when the box says it supports it (`localModel` store), and only
 * on an empty chat: a local chat stays local, so its turns never reach a
 * cloud model, and a stored chat never becomes local.
 */

export type AgentModeId = 'chat' | 'deep_research' | 'sudo' | 'local';

export interface AgentMode {
	id: AgentModeId;
	name: string;
	description: string;
	icon: string;
}

export const AGENT_MODES: AgentMode[] = [
	{
		id: 'chat',
		name: 'Chat',
		description: 'Asks before changing anything',
		icon: 'mynaui:message'
	},
	{
		id: 'deep_research',
		name: 'Deep research',
		description: 'Investigates your records and the web',
		icon: 'mynaui:search'
	},
	{
		id: 'sudo',
		name: 'Sudo',
		description: 'Full access to the server, asks before deleting anything',
		icon: 'mynaui:terminal'
	},
	{
		id: 'local',
		name: 'Local',
		description: "A small model on your server's NPU",
		icon: 'mynaui:chip'
	}
];

/**
 * The composer shows the one mode as two controls. Depth steps through the
 * modes below, lightest first; access is ask-first or full access (sudo).
 * The server takes a single mode, so choosing on one control resets the
 * other: full access runs at the balanced depth.
 */
export const DEPTH_ORDER: AgentModeId[] = ['local', 'chat', 'deep_research'];

/** What the depth control calls each of its stops. */
export const DEPTH_NAMES: Partial<Record<AgentModeId, string>> = {
	local: 'Local',
	chat: 'Balanced',
	deep_research: 'Deep research'
};

/** The depth stops this chat offers, lightest first. */
export function depthModes(modes: AgentMode[]): AgentMode[] {
	return DEPTH_ORDER.map((id) => modes.find((m) => m.id === id)).filter(
		(m): m is AgentMode => m !== undefined
	);
}

export function getModeById(id: AgentModeId): AgentMode | undefined {
	return AGENT_MODES.find((m) => m.id === id);
}

/** The modes this chat may switch between. Local needs the box's support. */
export function availableModes(opts: { localSupported: boolean }): AgentMode[] {
	return AGENT_MODES.filter((m) => m.id !== 'local' || opts.localSupported);
}

/** The mode after `id` among `modes`, wrapping — what Shift+Tab moves to. */
export function nextMode(id: AgentModeId, modes: AgentMode[] = AGENT_MODES): AgentModeId {
	const i = modes.findIndex((m) => m.id === id);
	return modes[(i + 1) % modes.length].id;
}
