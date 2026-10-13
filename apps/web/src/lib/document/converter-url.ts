/**
 * Where the built converter is served from (`converter.ts`). Imported only
 * when the converter loads, so tests, which read the module from disk, never
 * resolve the asset.
 */

export { default } from './converter.wasm?url';
