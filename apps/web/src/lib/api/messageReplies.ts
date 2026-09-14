/**
 * Replies from the record — `app_message_replies` over the box API.
 *
 * The box drafts a reply to a message thread and holds it as a pending row.
 * A device shows the draft, sends it (or not), and reports back here. The
 * drafting itself is the `message_reply` applet; asking for one on demand is
 * a manual run of it.
 */

import { apiGet, apiSend } from './client';

export type MessageReplyStatus = 'pending' | 'sent' | 'answered' | 'dismissed' | 'expired';

export interface MessageReply {
	id: string;
	/** The chat GUID from chat.db — what macOS Messages sends to. */
	thread_id: string;
	ask_stream_id: string;
	ask_text: string;
	from_handle: string;
	from_name: string | null;
	is_group: boolean;
	draft: string;
	rationale: string | null;
	trigger: 'auto' | 'manual';
	status: MessageReplyStatus;
	sent_text: string | null;
	sent_at: string | null;
	created_at: string;
	updated_at: string;
}

/** The applet's id is derived from its directory: `applet_<dir>`. */
const REPLY_APPLET_ID = 'applet_message_reply';

export function listPendingMessageReplies(): Promise<MessageReply[]> {
	return apiGet<MessageReply[]>('/message-replies/pending');
}

export function getMessageReply(id: string): Promise<MessageReply> {
	return apiGet<MessageReply>(`/message-replies/${encodeURIComponent(id)}`);
}

/** The device sent it; `sentText` is what went out, edits included. */
export function markMessageReplySent(id: string, sentText: string): Promise<MessageReply> {
	return apiSend<MessageReply>('POST', `/message-replies/${encodeURIComponent(id)}/sent`, {
		sent_text: sentText,
	});
}

export function dismissMessageReply(id: string): Promise<MessageReply> {
	return apiSend<MessageReply>('POST', `/message-replies/${encodeURIComponent(id)}/dismiss`);
}

/**
 * Ask the box to draft for a thread — or, with no thread, for whichever one
 * most recently messaged the owner. Returns as soon as the run is queued; the
 * draft, if any, shows up as a pending row.
 */
export function requestMessageReply(threadId?: string): Promise<{ run_id: string }> {
	return apiSend<{ run_id: string }>('POST', `/applets/${REPLY_APPLET_ID}/run`, {
		payload: threadId ? { thread_id: threadId } : {},
	});
}
