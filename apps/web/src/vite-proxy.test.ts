/**
 * The dev server's proxy to the box (`vite.config.ts`). Every socket on the
 * box refuses a page from another origin (`refuse_foreign_socket`), and the
 * dev server's page dials the box's sockets through this proxy. It rewrote
 * every handshake's Origin to the box's own, so the box took any page open
 * in the browser for its own page, from loopback, as the owner: a page on
 * any site could open the page socket, or the terminal and its shell. The
 * Origin goes to the box as the browser sent it, and `make dev` lists the
 * dev server's own origin for the page socket alone.
 */

import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { createServer, type ProxyOptions, type UserConfig, type ViteDevServer } from 'vite';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');

const closing: (() => Promise<void>)[] = [];

afterEach(async () => {
	for (const close of closing.splice(0)) await close();
});

/** A stand-in box on a free port, recording each socket handshake it is sent, and refusing it. */
async function standInBox(): Promise<{ port: number; handshakes: http.IncomingHttpHeaders[] }> {
	const handshakes: http.IncomingHttpHeaders[] = [];
	const box = http.createServer((_req, res) => res.end());
	box.on('upgrade', (req, socket) => {
		handshakes.push(req.headers);
		socket.end('HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n');
	});
	await new Promise<void>((resolve) => box.listen(0, '127.0.0.1', resolve));
	closing.push(() => new Promise((resolve) => box.close(() => resolve())));
	return { port: (box.address() as net.AddressInfo).port, handshakes };
}

/** The dev server, with `vite.config.ts`'s socket proxy pointed at `boxPort`. */
async function devServer(boxPort: number): Promise<number> {
	const defined = (await import('../vite.config')).default as unknown;
	const config = (typeof defined === 'function' ? await defined({ command: 'serve', mode: 'development' }) : defined) as UserConfig;
	const ws = config.server?.proxy?.['/ws'] as ProxyOptions;
	const server: ViteDevServer = await createServer({
		configFile: false,
		logLevel: 'silent',
		server: { host: '127.0.0.1', port: 0, strictPort: false, proxy: { '/ws': { ...ws, target: `http://127.0.0.1:${boxPort}` } } },
	});
	await server.listen();
	closing.push(() => server.close());
	return (server.httpServer!.address() as net.AddressInfo).port;
}

/** Send a socket handshake to `port` at `at`, from a page at `origin`, and wait for its answer. */
function handshake(port: number, at: string, origin: string): Promise<void> {
	return new Promise((resolve, reject) => {
		const socket = net.connect(port, '127.0.0.1', () =>
			socket.write(
				`GET ${at} HTTP/1.1\r\nHost: localhost:${port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
					`Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nOrigin: ${origin}\r\n\r\n`,
			),
		);
		socket.once('data', () => {
			socket.destroy();
			resolve();
		});
		socket.once('close', () => resolve());
		socket.once('error', reject);
	});
}

describe('the dev server’s socket proxy', () => {
	it('sends the box every page’s Origin as the browser sent it', async () => {
		const box = await standInBox();
		const port = await devServer(box.port);
		for (const [at, origin] of [
			['/ws/terminal', 'https://evil.example'],
			['/ws/yjs/page_1', 'http://localhost:8888'],
			['/ws/yjs/page_1', `http://localhost:${port}`],
		]) {
			await handshake(port, at, origin);
			expect(box.handshakes.at(-1)?.origin, `${at} from ${origin}`).toBe(origin);
		}
		expect(box.handshakes).toHaveLength(3);
	}, 30_000);

	it('lists the dev server’s own origin for the page socket, where `make dev` starts the box', () => {
		const makefile = fs.readFileSync(path.join(ROOT, 'Makefile'), 'utf8');
		const recipe = makefile.slice(makefile.indexOf('\ndev-core:'), makefile.indexOf('\nseed:'));
		expect(recipe).toContain('VIRTUES_PUBLIC_ORIGINS="http://localhost:$(DEV_WEB_PORT),http://127.0.0.1:$(DEV_WEB_PORT)"');
	});
});
