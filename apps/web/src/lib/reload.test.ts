// @vitest-environment happy-dom
/**
 * A reload ends a page's file still on its way, its only copy, and in the
 * Mac and iOS apps nothing asks first: their web views show no
 * `beforeunload` prompt. "Reload for the latest", "Reload the app", the
 * reload after an update and the app's own reloads for a new build or a
 * stale chunk each reloaded over one without a word.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

let sendEnds: () => Promise<number> = () => new Promise(() => {});

vi.mock('svelte-sonner', () => ({ toast: vi.fn() }));
vi.mock('$lib/api/client', () => {
	class UploadError extends Error {
		constructor(
			message: string,
			readonly status: number,
		) {
			super(message);
		}
		get refused(): boolean {
			return this.status >= 400 && this.status < 500 && this.status !== 408 && this.status !== 429;
		}
	}
	return {
		deadline: () => undefined,
		getMediaUploads: vi.fn(async () => ({})),
		announcePageFiles: vi.fn(async () => undefined),
		getNamedUploads: vi.fn(async (ids: string[]) => Object.fromEntries(ids.map((id) => [id, true]))),
		sendPageFile: vi.fn(async (id: string) => {
			const status = await sendEnds();
			if (status >= 200 && status < 300) return { id };
			throw new UploadError(`Your server answered ${status}.`, status);
		}),
		UploadError,
	};
});

const { toast } = await import('svelte-sonner');
const { resetUploads, startUpload } = await import('$lib/document/uploads');
const { reloadApp, reloadOnceQuiet, uploadsHold } = await import('./reload');
const { reloadForStaleChunk } = await import('$lib/tabs/lazy');

const SRC = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

function sources(dir: string): string[] {
	return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
		const at = path.join(dir, e.name);
		if (e.isDirectory()) return sources(at);
		return /\.(ts|svelte)$/.test(e.name) && !/\.test\.ts$/.test(e.name) ? [at] : [];
	});
}

let reloads = 0;
let hidden = false;
let release: (status: number) => void = () => {};

beforeEach(() => {
	resetUploads();
	reloads = 0;
	hidden = false;
	sendEnds = () => new Promise((settle) => (release = settle));
	vi.mocked(toast).mockClear();
	vi.spyOn(window.location, 'reload').mockImplementation(() => {
		reloads++;
	});
	vi.spyOn(document, 'hidden', 'get').mockImplementation(() => hidden);
});

afterEach(() => {
	resetUploads();
	vi.restoreAllMocks();
});

const photo = () => new File(['bytes'], 'harbour.png', { type: 'image/png' });

describe('a reload, with a page file on its way', () => {
	it('goes through one check, wherever the app reloads', () => {
		// Start over revokes this device: no file of its can reach the server.
		const anyway = ['lib/reload.ts', 'lib/components/settings/ReopenSection.svelte'];
		const direct = sources(SRC)
			.filter((file) => /location\.reload\(/.test(fs.readFileSync(file, 'utf8')))
			.map((file) => path.relative(SRC, file))
			.filter((file) => !anyway.includes(file));
		expect(direct).toEqual([]);
	});

	// A file the server could not store held the reload with nothing to
	// do but wait, and the toast promised an upload that might never come.
	it('the person asked for waits, says why, and goes anyway if they choose', () => {
		startUpload('file_00c0ffee00c0ffee', photo());
		expect(reloadApp()).toBe(false);
		expect(reloads).toBe(0);
		const [title, said] = vi.mocked(toast).mock.calls.at(-1)! as [string, { action: { label: string; onClick: () => void } }];
		expect(title).toBe('Not reloaded');
		expect(said.action.label).toBe('Reload anyway');
		said.action.onClick();
		expect(reloads).toBe(1);

		let relaunched = 0;
		expect(uploadsHold('relaunch', () => relaunched++)).toBe(true);
		const [again, offered] = vi.mocked(toast).mock.calls.at(-1)! as [string, { action: { label: string; onClick: () => void } }];
		expect(again).toBe('Not relaunched');
		expect(offered.action.label).toBe('Relaunch anyway');
		offered.action.onClick();
		expect(relaunched).toBe(1);
		resetUploads();
		expect(reloadApp()).toBe(true);
		expect(reloads).toBe(2);
	});

	it('the app makes on its own waits until the file is stored', async () => {
		startUpload('file_00c0ffee00c0ffee', photo());
		hidden = true;
		const stop = reloadOnceQuiet(() => document.hidden);
		expect(reloads).toBe(0);
		release(201);
		await vi.waitFor(() => expect(reloads).toBe(1));
		stop();
	});

	it('for a stale chunk is not made', async () => {
		vi.spyOn(window, 'fetch').mockImplementation(async () => new Response('{}', { status: 200 }));
		sessionStorage.clear();
		startUpload('file_00c0ffee00c0ffee', photo());
		expect(await reloadForStaleChunk()).toBe(false);
		expect(reloads).toBe(0);
		// Stored, the app takes up its build.
		release(201);
		await vi.waitFor(() => expect(reloadForStaleChunk()).resolves.toBe(true));
		expect(reloads).toBe(1);
	});
});
