/**
 * vitest's global setup: the editor's converter, built from the crate before
 * any test pastes through it (scripts/build-converter.mjs).
 */

import { buildConverter } from './build-converter.mjs';

export default function setup() {
	buildConverter();
}
