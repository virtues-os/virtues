/**
 * Agent Mode Configuration
 *
 * The modes the composer cycles through with Shift+Tab. The id goes to the
 * server as `agentMode`, which picks the turn's tools and prompt
 * (`tools::get_tools_for_agent_mode`, `agent::prompt`).
 *
 * `sudo` is the owner's bypass: a shell on the server with passwordless sudo,
 * and nothing asks before it runs. It lasts for one chat and starts off every
 * time a chat opens.
 */

export type AgentModeId = 'chat' | 'deep_research' | 'sudo';

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
	}
];

export function getModeById(id: AgentModeId): AgentMode | undefined {
	return AGENT_MODES.find((m) => m.id === id);
}

/** The mode after `id`, wrapping — what Shift+Tab moves to. */
export function nextMode(id: AgentModeId): AgentModeId {
	const i = AGENT_MODES.findIndex((m) => m.id === id);
	return AGENT_MODES[(i + 1) % AGENT_MODES.length].id;
}
