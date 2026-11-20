/**
 * The servers on this server's Virtues account, and removing one.
 *
 * Any server on an account can use its AI credit, so an owner needs a way to
 * remove one they don't recognize, such as a box attached by mistake. The
 * server asks atlas with its own key; removing is owner-proven because it
 * takes another server off Virtues AI.
 */
import { apiGet, apiSendProven } from "./client";

export type AccountServer = {
	/** Opaque: the hash of that server's key. */
	id: string;
	linked_at: string;
	this_server: boolean;
};

type ListReply = { servers?: AccountServer[]; unlinked?: boolean; error?: string; code?: string };

export async function listAccountServers(): Promise<ListReply> {
	return apiGet<ListReply>("/billing/servers");
}

export async function removeAccountServer(id: string): Promise<{ ok?: boolean; error?: string; code?: string }> {
	return apiSendProven("POST", "/billing/servers/remove", { id });
}
