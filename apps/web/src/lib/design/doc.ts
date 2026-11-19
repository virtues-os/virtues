/**
 * The design system's text, read from the repo at build time, so the
 * /design page and agents/build/design.md are one file.
 */

import source from '../../../../../agents/build/design.md?raw';

const REPO_DOCS = 'https://github.com/virtues-os/virtues/blob/wave/agents/build/';

/**
 * The file as the page renders it: a link to another doc in agents/build
 * points at that doc on GitHub, since the page cannot open a repo file.
 */
export const designDoc: string = source.replace(
	/\]\((?!https?:|#|\/)([^)\s]+\.md)(#[^)\s]*)?\)/g,
	(_m, file: string, hash = '') => `](${REPO_DOCS}${file}${hash})`
);
