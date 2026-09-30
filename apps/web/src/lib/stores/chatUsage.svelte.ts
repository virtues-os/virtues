/**
 * Chat usage: a counter per chat, bumped when a turn has been written, so
 * every view of that chat's usage reads it again. The chat tab and the
 * context view are separate tabs with separate fetches, and the context view
 * otherwise reads only when it opens — before a stopped turn is saved, which
 * showed the person's message and none of the turn behind it.
 */

const written = $state<Record<string, number>>({});

export const chatUsage = {
	/** A turn in this chat has been saved; usage views should read again. */
	turnWritten(chatId: string) {
		written[chatId] = (written[chatId] ?? 0) + 1;
	},
	/** Read inside an effect to re-run it when the chat's usage changes. */
	version(chatId: string): number {
		return written[chatId] ?? 0;
	},
};
