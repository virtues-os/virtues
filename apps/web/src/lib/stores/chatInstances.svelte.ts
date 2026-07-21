/**
 * Chat Instance Store
 * 
 * Manages Chat instances by conversationId so they persist across component
 * remounts and tab moves. This allows streaming to continue even when
 * tabs are dragged between panes or split mode is toggled.
 */

import { Chat } from '@ai-sdk/svelte';
import { DefaultChatTransport, type ChatTransport } from 'ai';
import { subscriptionStore } from '$lib/stores/subscription.svelte';
import type { CheckpointMessage } from '$lib/types/chat';

// --- Streaming reactivity helpers (see replaceMessage override below) ---------
//
// The AI SDK keeps ONE live message object per response and mutates its parts in
// place on every token, then calls replaceMessage(index, liveMessage). We keep a
// DECOUPLED snapshot in the reactive store and, on each delta, sync only what
// actually changed THROUGH the proxy — so a growing text part re-renders just its
// own node instead of the whole message subtree.

/** Decouple a part from the SDK's live object (it mutates parts in place). */
function snapshotPart(part: any): any {
    try {
        return structuredClone(part);
    } catch {
        return { ...part };
    }
}

/** Decouple a whole message (shallow fields + cloned parts). */
function snapshotMessage(message: any): any {
    return { ...message, parts: (message.parts ?? []).map(snapshotPart) };
}

function partsEqual(a: any, b: any): boolean {
    try {
        return JSON.stringify(a) === JSON.stringify(b);
    } catch {
        return false;
    }
}

// Coarse signature of a non-text part, used to skip the JSON compare for settled
// tool parts on every text delta. Source part refs are stable across deltas, so a
// WeakMap keyed by the SDK's live part lets us short-circuit unchanged parts.
const lastPartSig = new WeakMap<object, string>();
function partSignature(part: any): string {
    return `${part.type}|${part.state ?? ''}|${part.errorText !== undefined}|${part.output !== undefined}`;
}

/**
 * Sync the SDK's live `src` message into the decoupled, proxied `target` already
 * in the store. Only changed fields are written, so reactivity is fine-grained:
 * streaming text mutates a single string; tool/structural parts are replaced only
 * when they actually change.
 */
function syncMessageInPlace(target: any, src: any): void {
    // Top-level scalar/ref fields that can change mid-stream (status, metadata…).
    for (const key of Object.keys(src)) {
        if (key === 'parts' || key === 'id') continue;
        if (target[key] !== src[key]) target[key] = src[key];
    }

    const sParts = src.parts ?? [];
    const tParts = target.parts;

    for (let i = 0; i < sParts.length; i++) {
        const sp = sParts[i];
        const tp = tParts[i];

        if (!tp || tp.type !== sp.type) {
            tParts[i] = snapshotPart(sp); // new or type-changed part
            continue;
        }

        if (sp.type === 'text' || sp.type === 'reasoning') {
            // Hot path: only the growing text changes — mutate just that string.
            if (tp.text !== sp.text) tp.text = sp.text;
        } else {
            // Tool/other parts update rarely. Skip the JSON compare when this
            // source part hasn't changed shape since we last synced it.
            const sig = partSignature(sp);
            if (lastPartSig.get(sp) === sig) continue;
            lastPartSig.set(sp, sig);
            if (!partsEqual(tp, sp)) tParts[i] = snapshotPart(sp);
        }
    }

    if (tParts.length > sParts.length) tParts.length = sParts.length;
}
// -----------------------------------------------------------------------------

interface ChatInstanceEntry {
    chat: Chat;
    refCount: number; // Number of tabs/views referencing this instance
    createdAt: number;
    cleanupTimeout?: ReturnType<typeof setTimeout>;
    lastThoughtSignature?: string;
}

/** Live status of one Deep Research subagent, from transient `data-subagent` events. */
export interface SubagentStatus {
    /** Unique per-dispatch id; with subagentId it namespaces workers across dispatch rounds. */
    dispatchId: number;
    subagentId: number;
    title: string;
    model: string;
    status: 'thinking' | 'done' | 'failed';
    tokens: number;
}

interface ActivePageContext {
    page_id: string;
    page_title?: string;
    content?: string; // Current content from Yjs document
}

interface CreateChatConfig {
    conversationId: string;
    getModel: () => string; // Getter to always get current model
    getNotebookId: () => string | null; // Getter for space ID (null for system space)
    getActivePageContext?: () => ActivePageContext | null; // Getter for active page context (bound page)
    getPersona?: () => string; // Getter for selected persona (per-chat)
    getAgentMode?: () => string; // Getter for agent mode (agent, chat, research)
    getChatMode?: () => string; // Getter for retrieval scope: 'open' | 'scoped' (notebook chats)
    getTemporary?: () => boolean; // Getter for temporary/ghost mode (don't persist server-side)
}

class ChatInstanceStore {
    private instances = $state(new Map<string, ChatInstanceEntry>());
    // Live Deep Research subagent state, keyed by conversationId. Ephemeral — rebuilt each turn
    // from transient `data-subagent` events.
    private subagents = $state(new Map<string, SubagentStatus[]>());

    /** Current Deep Research subagents for a conversation (empty if none active). */
    getSubagents(conversationId: string): SubagentStatus[] {
        return this.subagents.get(conversationId) ?? [];
    }

    /** Clear the live subagent panel for a conversation (called at the start of each turn). */
    clearSubagents(conversationId: string) {
        if (this.subagents.has(conversationId)) this.subagents.set(conversationId, []);
    }

    /** Upsert one subagent's status from a `data-subagent` event. */
    private applySubagent(conversationId: string, s: SubagentStatus) {
        // Workers are keyed by (dispatchId, subagentId) so parallel ordering and multiple dispatch
        // rounds in one turn never collide. The list is cleared per turn (clearSubagents on send),
        // so within a turn we simply accumulate/update.
        const existing = (this.subagents.get(conversationId) ?? []).slice();
        const idx = existing.findIndex(
            (e) => e.dispatchId === s.dispatchId && e.subagentId === s.subagentId
        );
        if (idx >= 0) existing[idx] = s;
        else existing.push(s);
        existing.sort((a, b) => a.dispatchId - b.dispatchId || a.subagentId - b.subagentId);
        this.subagents.set(conversationId, existing);
    }

    /**
     * Get an existing Chat instance or create a new one.
     * Increments reference count.
     * 
     * @param config - Configuration including conversationId and getModel getter
     */
    getOrCreate(config: CreateChatConfig): Chat {
        const { conversationId, getModel, getNotebookId, getActivePageContext, getPersona, getAgentMode, getChatMode, getTemporary } = config;
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
                    const notebookId = getNotebookId();
                    const activePage = getActivePageContext?.();
                    const persona = getPersona?.() || 'default';
                    const agentMode = getAgentMode?.() || 'chat';
                    const chatMode = getChatMode?.() || 'open';
                    const temporary = getTemporary?.() || false;
                    const entry = this.instances.get(conversationId);
                    const thoughtSignature = entry?.lastThoughtSignature;

                    return {
                        body: {
                            chatId: conversationId,
                            model: getModel(),
                            agentId: 'auto',
                            messages,
                            persona,
                            agentMode,
                            // Retrieval scope for notebook chats: 'open' (whole
                            // graph, notebook up-weighted) or 'scoped' (grounded).
                            chatMode,
                            // Ghost/temporary chat — backend should skip persistence when true.
                            ...(temporary && { temporary: true }),
                            // User's timezone for temporal awareness (IANA format, e.g., "America/Los_Angeles")
                            timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
                            // The Space (room) this chat lives in — drives the agent's
                            // active-space context block and binds the chat on the server.
                            ...(notebookId && { notebookId }),
                            // Include active page context if a page is bound
                            ...(activePage && { activePage }),
                            // Include thought signature if available
                            ...(thoughtSignature && { thoughtSignature })
                        }
                    };
                }
            }),
            messages: [],
            onData: (dataPart) => {
                // Handle thought signature events (transient - only for state tracking)
                if (dataPart.type === 'data-thought-signature') {
                    const entry = this.instances.get(conversationId);
                    if (entry) {
                        entry.lastThoughtSignature = (dataPart.data as { signature: string }).signature;
                    }
                }
                // Handle Deep Research subagent events (transient - drives the live panel)
                else if (dataPart.type === 'data-subagent') {
                    this.applySubagent(conversationId, dataPart.data as SubagentStatus);
                }
                // Handle checkpoint events from auto-compaction (non-transient - persists in messages)
                else if (dataPart.type === 'data-checkpoint') {
                    const entry = this.instances.get(conversationId);
                    if (entry?.chat) {
                        const data = dataPart.data as {
                            version: number;
                            messagesSummarized: number;
                            summary: string;
                            timestamp: string;
                        };
                        const checkpointMessage: CheckpointMessage = {
                            id: dataPart.id || `checkpoint_${Date.now()}`,
                            role: 'checkpoint',
                            parts: [{
                                type: 'checkpoint',
                                version: data.version,
                                messagesSummarized: data.messagesSummarized,
                                summary: data.summary,
                                timestamp: data.timestamp,
                            }],
                        };
                        // Insert checkpoint message into chat for immediate display.
                        // The SDK types `messages` as UIMessage[]; a checkpoint is a
                        // synthetic render-only message, so cast at this boundary.
                        entry.chat.messages = [
                            ...entry.chat.messages,
                            checkpointMessage as unknown as (typeof entry.chat.messages)[number],
                        ];
                    }
                }
            },
            onError: (error) => {
                console.error(`[ChatInstances] Error in chat ${conversationId}:`, error);

                // A lapsed subscription / unrecognized key surfaces as these
                // codes from virtues-api (402/401) — refresh subscription state.
                if (/wallet_expired|subscription_inactive|unknown_key/.test(error.message ?? '')) {
                    subscriptionStore.check();
                }
            }
        });

        // Fix Svelte 5 streaming reactivity WITHOUT re-rendering the whole message
        // subtree on every token. SvelteChatState.replaceMessage() assigns the same
        // live object the SDK mutates in place, which the $state proxy skips on
        // `===`. The old fix spread into a new object each delta (new identity →
        // full-subtree re-render). Instead we keep a decoupled snapshot and sync
        // only what changed through the proxy, so the streaming text part updates
        // on its own and earlier paragraphs / tool cards / thinking block stay put.
        const internalState = (chat as any).state;
        const originalReplace = internalState.replaceMessage.bind(internalState);
        internalState.replaceMessage = (index: number, message: any) => {
            const existing = internalState.messages[index];
            if (
                existing &&
                existing.id === message.id &&
                existing !== message &&
                Array.isArray(existing.parts) &&
                Array.isArray(message.parts)
            ) {
                syncMessageInPlace(existing, message);
            } else {
                // New/replaced message, or no decoupled target yet → store a clone
                // so future deltas can be diffed against it.
                originalReplace(index, snapshotMessage(message));
            }
        };

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
                    this.subagents.delete(conversationId);
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
