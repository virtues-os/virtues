/**
 * Chat Instance Store
 * 
 * Manages Chat instances by conversationId so they persist across component
 * remounts and tab moves. This allows streaming to continue even when
 * tabs are dragged between panes or split mode is toggled.
 */

import { Chat } from '@ai-sdk/svelte';
import { DefaultChatTransport, type ChatTransport } from 'ai';

interface ChatInstanceEntry {
    chat: Chat;
    refCount: number; // Number of tabs/views referencing this instance
    createdAt: number;
    cleanupTimeout?: ReturnType<typeof setTimeout>;
    lastThoughtSignature?: string;
}

interface ActivePageContext {
    page_id: string;
    page_title?: string;
    content?: string; // Current content from Yjs document
}

interface CreateChatConfig {
    conversationId: string;
    getModel: () => string; // Getter to always get current model
    getSpaceId: () => string | null; // Getter for space ID (null for system space)
    getActivePageContext?: () => ActivePageContext | null; // Getter for active page context (bound page)
}

class ChatInstanceStore {
    private instances = $state(new Map<string, ChatInstanceEntry>());

    /**
     * Get an existing Chat instance or create a new one.
     * Increments reference count.
     * 
     * @param config - Configuration including conversationId and getModel getter
     */
    getOrCreate(config: CreateChatConfig): Chat {
        const { conversationId, getModel, getSpaceId, getActivePageContext } = config;
        const existing = this.instances.get(conversationId);

        if (existing) {
            // If pending cleanup, cancel it
            if (existing.cleanupTimeout) {
                clearTimeout(existing.cleanupTimeout);
                existing.cleanupTimeout = undefined;
            }
            existing.refCount++;
            return existing.chat;
        }

        // Create new Chat instance with transport that uses the getters
        const chat = new Chat({
            id: conversationId,
            transport: new DefaultChatTransport({
                api: '/api/chat',
                prepareSendMessagesRequest: ({ messages }) => {
                    const spaceId = getSpaceId();
                    const activePage = getActivePageContext?.();
                    const entry = this.instances.get(conversationId);
                    const thoughtSignature = entry?.lastThoughtSignature;

                    return {
                        body: {
                            chatId: conversationId,
                            model: getModel(),
                            agentId: 'auto',
                            messages,
                            // User's timezone for temporal awareness (IANA format, e.g., "America/Los_Angeles")
                            timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
                            // Only include spaceId if not null (null = system space, don't auto-add)
                            ...(spaceId && { spaceId }),
                            // Include active page context if a page is bound
                            ...(activePage && { activePage }),
                            // Include thought signature if available
                            ...(thoughtSignature && { thoughtSignature })
                        }
                    };
                }
            }),
            messages: [],
            onResponse: (response) => {
                // Extract thought signature from headers if available
                const sig = response.headers.get('x-gemini-thought-signature');
                if (sig) {
                    const entry = this.instances.get(conversationId);
                    if (entry) {
                        entry.lastThoughtSignature = sig;
                    }
                }
            },
            onChunk: (chunk) => {
                // Extract thought signature from custom event if available
                if (chunk.type === 'thought-signature') {
                    const entry = this.instances.get(conversationId);
                    if (entry) {
                        entry.lastThoughtSignature = chunk.signature;
                    }
                }
            },
            onError: (error) => {
                console.error(`[ChatInstances] Error in chat ${conversationId}:`, error);
            }
        });

        const entry: ChatInstanceEntry = {
            chat,
            refCount: 1,
            createdAt: Date.now()
        };

        this.instances.set(conversationId, entry);

        return chat;
    }

    /**
     * Get an existing Chat instance without creating.
     */
    get(conversationId: string): Chat | undefined {
        return this.instances.get(conversationId)?.chat;
    }

    /**
     * Check if an instance exists.
     */
    has(conversationId: string): boolean {
        return this.instances.has(conversationId);
    }

    /**
     * Release a reference to a Chat instance.
     * When refCount reaches 0, wait a grace period before destroying.
     */
    release(conversationId: string): void {
        const entry = this.instances.get(conversationId);
        if (!entry) return;

        entry.refCount--;

        if (entry.refCount <= 0) {
            // Start grace period before destruction
            entry.cleanupTimeout = setTimeout(() => {
                // Double check refCount didn't go back up
                if (entry.refCount <= 0) {
                    this.instances.delete(conversationId);
                }
            }, 1000);
        }
    }

    /**
     * Pre-populate a Chat instance with loaded messages.
     * Used when hydrating from server data.
     */
    setMessages(conversationId: string, messages: any[]): void {
        const chat = this.instances.get(conversationId)?.chat;
        if (chat) {
            chat.messages = messages;
        }
    }

    /**
     * Debug: get all active instances.
     */
    debug(): { conversationId: string; refCount: number; status: string }[] {
        return Array.from(this.instances.entries()).map(([id, entry]) => ({
            conversationId: id,
            refCount: entry.refCount,
            status: entry.chat.status
        }));
    }
}

export const chatInstances = new ChatInstanceStore();
