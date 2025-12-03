/**
 * Custom OpenTelemetry SpanExporter for PostgreSQL
 *
 * Captures AI SDK telemetry spans and persists them to the chat_sessions.trace column.
 * This allows us to have full observability into agent execution without external services.
 */
import type { SpanExporter, ReadableSpan } from '@opentelemetry/sdk-trace-base';
import { ExportResultCode, type ExportResult } from '@opentelemetry/core';
import { getPool } from '$lib/server/db';
import type { SpanTrace } from '$lib/types/Trace';

export class PostgresSpanExporter implements SpanExporter {
	/**
	 * Export spans to PostgreSQL
	 * Only processes top-level AI spans (ai.streamText, ai.generateText)
	 */
	export(
		spans: ReadableSpan[],
		resultCallback: (result: ExportResult) => void
	): void {
		this.processSpans(spans)
			.then(() => {
				resultCallback({ code: ExportResultCode.SUCCESS });
			})
			.catch((error) => {
				console.error('[Telemetry] Failed to export spans:', error);
				resultCallback({ code: ExportResultCode.FAILED });
			});
	}

	private async processSpans(spans: ReadableSpan[]): Promise<void> {
		for (const span of spans) {
			// Only process top-level AI spans
			if (!span.name.startsWith('ai.streamText') && !span.name.startsWith('ai.generateText')) {
				continue;
			}

			const attrs = span.attributes;
			const sessionId = attrs['ai.telemetry.metadata.sessionId'] as string | undefined;

			if (!sessionId) {
				continue;
			}

			const trace = this.buildTrace(span);
			await this.saveTrace(sessionId, trace);
		}
	}

	/**
	 * Build a SpanTrace from an OpenTelemetry span
	 */
	private buildTrace(span: ReadableSpan): SpanTrace {
		const attrs = span.attributes;

		// Calculate duration in milliseconds
		// startTime and endTime are [seconds, nanoseconds] tuples
		const startMs = span.startTime[0] * 1000 + span.startTime[1] / 1_000_000;
		const endMs = span.endTime[0] * 1000 + span.endTime[1] / 1_000_000;
		const durationMs = endMs - startMs;

		return {
			spanId: span.spanContext().spanId,
			traceId: span.spanContext().traceId,
			timestamp: new Date(startMs).toISOString(),
			durationMs,

			// Orchestration context (from our metadata)
			agentId: (attrs['ai.telemetry.metadata.agentId'] as string) || 'unknown',
			routingReason: (attrs['ai.telemetry.metadata.routingReason'] as string) || '',
			wasExplicit: (attrs['ai.telemetry.metadata.wasExplicit'] as boolean) || false,

			// Model info
			model: (attrs['ai.model.id'] as string) || (attrs['gen_ai.request.model'] as string) || '',
			provider: (attrs['ai.model.provider'] as string) || (attrs['gen_ai.system'] as string) || '',

			// What was sent to the model (captured by AI SDK)
			prompt: attrs['ai.prompt'] as string | undefined,
			promptMessages: attrs['ai.prompt.messages'] as string | undefined,
			promptTools: attrs['ai.prompt.tools'] as string | undefined,

			// Response
			responseText: attrs['ai.response.text'] as string | undefined,
			responseToolCalls: attrs['ai.response.toolCalls'] as string | undefined,
			finishReason: (attrs['ai.response.finishReason'] as string) || (attrs['gen_ai.response.finish_reasons'] as string) || undefined,

			// Usage
			promptTokens: attrs['ai.usage.promptTokens'] as number | undefined,
			completionTokens: attrs['ai.usage.completionTokens'] as number | undefined,

			// Timing
			msToFirstChunk: attrs['ai.response.msToFirstChunk'] as number | undefined,
			msToFinish: attrs['ai.response.msToFinish'] as number | undefined
		};
	}

	/**
	 * Save trace to database by appending to existing spans array
	 */
	private async saveTrace(sessionId: string, trace: SpanTrace): Promise<void> {
		const pool = getPool();

		// Use PostgreSQL JSONB operators to append to the spans array
		// Creates the structure if it doesn't exist
		await pool.query(
			`
			UPDATE app.chat_sessions
			SET trace = jsonb_set(
				COALESCE(trace, '{"spans":[]}'::jsonb),
				'{spans}',
				COALESCE(trace->'spans', '[]'::jsonb) || $1::jsonb
			),
			updated_at = NOW()
			WHERE id = $2
			`,
			[JSON.stringify([trace]), sessionId]
		);
	}

	/**
	 * Force flush any pending spans (no-op for immediate export)
	 */
	forceFlush(): Promise<void> {
		return Promise.resolve();
	}

	/**
	 * Shutdown the exporter
	 */
	shutdown(): Promise<void> {
		return Promise.resolve();
	}
}
