/**
 * Agent Mode Configuration
 *
 * The modes the composer cycles through with Shift+Tab. The id goes to the
 * server as `agentMode`, which picks the turn's tools and prompt
 * (`ChatMode::tools`, `agent::prompt`).
 *
 * `sudo` is the owner's bypass: a shell on the server with passwordless sudo,
 * and nothing asks before it runs. It lasts for one chat and starts off every
 * time a chat opens.
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
	/** Color of the composer chip (null = the quiet default) */
	color: string | null;
}

export const AGENT_MODES: AgentMode[] = [
	{
		id: 'chat',
		name: 'Chat',
		description: 'Asks before changing anything',
		icon: 'ri:chat-3-line',
		color: null
	},
	{
		id: 'deep_research',
		name: 'Deep research',
		description: 'Investigates your records and the web',
		icon: 'ri:search-eye-line',
		color: 'var(--color-info)'
	},
	{
		id: 'sudo',
		name: 'Sudo',
		description: 'Full access to the server, nothing asks first',
		icon: 'ri:terminal-box-line',
		color: 'var(--color-error)'
	},
	{
		id: 'local',
		name: 'Local',
		description: "A small model on your server's NPU",
		icon: 'ri:cpu-line',
		color: null
	}
];

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
