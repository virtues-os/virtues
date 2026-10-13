/**
 * `sendPageFile`: a page's file counts as stored only at the address its
 * block holds.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';
import { sendPageFile, UploadError } from './client';

const ID = 'file_00c0ffee00c0ffee';

/** An XMLHttpRequest that answers every send with `status` and `body`. */
function answering(status: number, body: unknown) {
	return class {
		status = 0;
		responseText = '';
		upload: { onprogress: ((e: ProgressEvent) => void) | null } = { onprogress: null };
		onload: (() => void) | null = null;
		onerror: (() => void) | null = null;
		open() {}
		send() {
			this.status = status;
			this.responseText = JSON.stringify(body);
			queueMicrotask(() => this.onload?.());
		}
	};
}

const file = () => new File(['bytes'], 'harbour.png', { type: 'image/png' });

afterEach(() => vi.unstubAllGlobals());

describe('sendPageFile', () => {
	it('resolves with the file stored at the id it was sent to', async () => {
		vi.stubGlobal('XMLHttpRequest', answering(201, { id: ID, url: `/api/drive/files/${ID}/download` }));
		await expect(sendPageFile(ID, file())).resolves.toMatchObject({ id: ID });
	});

	// A box from before page files took their id stored the bytes under one
	// of its own, answered 201, and the upload was marked done while the
	// block's address named nothing on any device.
	it('refuses a file stored under another id', async () => {
		vi.stubGlobal('XMLHttpRequest', answering(201, { id: 'file_0123456789abcdef' }));
		const error = await sendPageFile(ID, file()).catch((e: unknown) => e);
		expect(error).toBeInstanceOf(UploadError);
		expect((error as UploadError).refused).toBe(true);
		expect((error as UploadError).message).toContain('harbour.png');
	});

	it('says why a refusal came, and tells a refusal from a stop it may recover from', async () => {
		vi.stubGlobal('XMLHttpRequest', answering(413, { error: 'harbour.png is over 100 MB.' }));
		const refused = (await sendPageFile(ID, file()).catch((e: unknown) => e)) as UploadError;
		expect([refused.message, refused.refused]).toEqual(['harbour.png is over 100 MB.', true]);
		vi.stubGlobal('XMLHttpRequest', answering(507, { error: 'Your server is out of space.' }));
		const full = (await sendPageFile(ID, file()).catch((e: unknown) => e)) as UploadError;
		expect([full.message, full.refused]).toEqual(['Your server is out of space.', false]);
	});
});
