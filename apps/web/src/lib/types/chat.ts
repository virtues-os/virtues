import type { UIMessage } from 'ai';

/**
 * A synthetic compaction checkpoint the AI SDK does not model as a message.
 * Emitted by the box's auto-compaction (`data-checkpoint`) and inserted into
 * the chat stream purely so it can be rendered inline in the transcript.
 */
export interface CheckpointMessage {
	id: string;
	role: 'checkpoint';
	parts: Array<{
		type: 'checkpoint';
		version: number;
		messagesSummarized: number;
		summary: string;
		timestamp: string;
	}>;
}

/**
 * A message as rendered in the chat view: either a real SDK {@link UIMessage}
 * or a {@link CheckpointMessage}. The SDK's `Chat.messages` array is typed as
 * `UIMessage[]`, so inserting a checkpoint requires a cast at that boundary —
 * this union documents what actually flows through the transcript.
 */
export type AppMessage = UIMessage | CheckpointMessage;
