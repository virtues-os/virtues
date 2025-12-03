/**
 * OpenTelemetry setup for AI SDK telemetry
 *
 * Initializes a tracer provider with a custom Postgres exporter
 * to capture and persist AI SDK spans to our database.
 */
import { NodeTracerProvider } from '@opentelemetry/sdk-trace-node';
import { SimpleSpanProcessor } from '@opentelemetry/sdk-trace-base';
import { PostgresSpanExporter } from './postgres-exporter';

let provider: NodeTracerProvider | null = null;

/**
 * Initialize the OpenTelemetry tracer provider
 * Safe to call multiple times - returns existing provider if already initialized
 */
export function initTelemetry(): NodeTracerProvider {
	if (provider) return provider;

	// Create provider with span processor configured via spanProcessors array (new API)
	provider = new NodeTracerProvider({
		spanProcessors: [new SimpleSpanProcessor(new PostgresSpanExporter())]
	});
	provider.register();

	return provider;
}

/**
 * Get the tracer for AI SDK telemetry
 * Automatically initializes the provider if not already done
 */
export function getTracer() {
	return initTelemetry().getTracer('ariata-chat');
}
