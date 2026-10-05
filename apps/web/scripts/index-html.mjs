/**
 * Write `build/index.html`, a copy of SvelteKit's SPA fallback `200.html`.
 *
 * Every copy of the web app needs it: the Mac and iPhone serve "/" from their
 * own copy, and their over-the-air updater refuses a bundle without it
 * (`is_usable` in src-tauri/src/web_bundle.rs). The box hands out its own build
 * as that bundle, so it has to be written here, by the one build every copy
 * comes from, and before write-bundle-manifest.mjs hashes the tree.
 */
import { copyFileSync } from 'node:fs';
import { join } from 'node:path';

const BUILD_DIR = join(import.meta.dirname, '..', 'build');
copyFileSync(join(BUILD_DIR, '200.html'), join(BUILD_DIR, 'index.html'));
