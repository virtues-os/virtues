/**
 * Tab system module
 * 
 * Provides type-safe tab management with registry-based dynamic dispatch.
 */

export * from './types';
export { tabRegistry, parseRoute } from './registry';
export { serializeToUrl, deserializeFromUrl, hasUrlTabParams } from './urlSerializer';
