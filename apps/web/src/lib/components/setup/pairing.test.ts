import { describe, expect, it } from 'vitest';
import { pairError, splitPasted } from './pairing';

describe('splitPasted', () => {
	it('splits a line from virtues pair into address and code', () => {
		expect(splitPasted('  http://192.168.1.20:8000   123 456 ')).toEqual({
			address: 'http://192.168.1.20:8000',
			code: '123456',
		});
		expect(splitPasted('http://[fd00::5]:8000 123456')).toEqual({
			address: 'http://[fd00::5]:8000',
			code: '123456',
		});
	});

	it('leaves an address alone, or a computer handoff', () => {
		expect(splitPasted('http://192.168.1.20:8000')).toBeNull();
		expect(splitPasted('{"v":1,"seed":"x","box_json":"{}"}')).toBeNull();
	});
});

describe('pairError', () => {
	const say = (raw: string) => pairError(raw, { address: 'http://192.168.1.20:8000', device: 'this Mac' });

	it('says an expired code is expired', () => {
		expect(say('pair failed: 401 Unauthorized — {"error":"invalid_or_expired_token"}').text).toMatch(
			/run virtues pair for a new one/,
		);
	});

	it('names the address without its scheme when the server is out of reach', () => {
		expect(say('error sending request: operation timed out').text).toMatch(/^This Mac couldn't reach 192\.168\.1\.20:8000/);
	});

	it('marks a local port clash as already paired', () => {
		const r = say('bind 127.0.0.1:7117: Address already in use (os error 48)');
		expect(r.paired).toBe(true);
	});

	it('falls back to a fresh code', () => {
		expect(say('something else').text).toMatch(/fresh code/);
	});
});
