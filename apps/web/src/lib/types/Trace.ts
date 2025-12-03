/**
 * Trace Types for Agent Execution Observability
 *
 * OpenTelemetry-based trace data from AI SDK.
 * These types define the structure for capturing full agent execution context,
 * including system prompts, routing decisions, model input, and token usage.
 */

/**
 * Complete trace for a conversation session
 * Contains one SpanTrace per AI SDK call (streamText/generateText)
 */
export interface ConversationTrace {
	spans: SpanTrace[];
}

/**
 * Trace data for a single AI SDK span
 * Captured automatically via experimental_telemetry
 */
export interface SpanTrace {
	/** OpenTelemetry span ID */
	spanId: string;

	/** OpenTelemetry trace ID */
	traceId: string;

	/** When this span started */
	timestamp: string;

	/** Duration in milliseconds */
	durationMs: number;

	/** Which agent handled this exchange */
	agentId: string;

	/** Why this agent was selected */
	routingReason: string;

	/** True if user explicitly selected the agent, false if auto-routed */
	wasExplicit: boolean;

	/** Model used for this exchange */
	model: string;

	/** AI provider (e.g., "anthropic") */
	provider: string;

	/** Full prompt sent to the model (ai.prompt) */
	prompt?: string;

	/** Full messages array sent to the model (ai.prompt.messages - stringified JSON) */
	promptMessages?: string;

	/** Tools available to the model (ai.prompt.tools - stringified JSON) */
	promptTools?: string;

	/** Model's text response */
	responseText?: string;

	/** Tool calls made by the model (stringified JSON) */
	responseToolCalls?: string;

	/** Why the model stopped generating */
	finishReason?: string;

	/** Number of tokens in the prompt */
	promptTokens?: number;

	/** Number of tokens in the completion */
	completionTokens?: number;

	/** Time to first chunk in milliseconds */
	msToFirstChunk?: number;

	/** Time to finish in milliseconds */
	msToFinish?: number;
}
