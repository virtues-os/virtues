// Tests for the airlock's routing rules.
//
//     node --test "apps/web/src-tauri/ui/*.test.mjs"    (from the repo root)
//
// The QUOTED GLOB matters: `node --test <dir>` resolves the directory as a
// module and fails with MODULE_NOT_FOUND rather than discovering anything.
//
// ## Why the test slices the HTML apart
//
// `connect.html` is THE airlock: compiled into the binary with `include_bytes!`
// and served before any bundle, so it cannot be shadowed by packaging or fail
// to fetch. Moving the router into its own file would buy conventional imports
// at the cost of a second fetch through the custom protocol — a second way for
// the one screen that must always work to come up blank. So the router stays
// inline behind `ROUTER:BEGIN`/`ROUTER:END` markers, and this reaches in.
//
// The cost of that choice is exactly one thing: if someone renames the markers,
// this fails loudly rather than silently testing nothing. `assert.ok` on the
// slice is what makes that true, and it is the first assertion in the file.
//
// ## What is worth testing here
//
// Not "does the router work": it is a few lines of `if`. What these pin are
// the RULES, each of which came from a real failure and none of which is
// obvious from reading the code:
//
//   · the launch hash and the injected paired flag yield ONE destination
//   · only an authed session opens the app; a refused key and no answer get
//     their own screens
//   · Setup's entries (`#existing`, `#address`, `#pair?…`) reach the right
//     screen, and a hash that names no server falls back to the address form
//   · a line from `virtues pair`, pasted or scanned, fills address and code,
//     and a computer's JSON handoff is never mistaken for one
//   · a pairing failure says which part failed, never the raw error
//
// A regression in any of these strands someone on their first run, and none
// of them would be caught by a type checker or noticed in review.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const html = readFileSync(join(here, 'connect.html'), 'utf8');

// The marker lines end in box-drawing rule characters, which are not JS — so
// take everything after the first newline of the BEGIN line.
const afterBegin = html.split('ROUTER:BEGIN')[1];
const slice = afterBegin?.slice(afterBegin.indexOf('\n')).split('ROUTER:END')[0];
assert.ok(
  slice && slice.includes('const Router'),
  'the ROUTER:BEGIN/END markers moved or vanished — this file is testing nothing',
);

// Evaluate the region in isolation. It touches no DOM and no `invoke` by
// construction; if that ever stops being true, this throws, which is the point.
// The NEWLINE is load-bearing: the slice ends partway through the END marker's
// comment line, so a `return` appended directly would sit inside that comment
// and the function would return undefined — silently, with no syntax error.
const Router = new Function(`${slice}\n; return Router;`)();
assert.equal(typeof Router, 'object', 'the router region did not evaluate to an object');

// ── launch ──────────────────────────────────────────────────────────────────

test('the launch hash decides, and decides once', () => {
  assert.equal(Router.bootDestination({ hash: '#reset', injectedPaired: true }), 'forgot-us');
  assert.equal(Router.bootDestination({ hash: '#unreachable', injectedPaired: true }), 'unreachable');
  assert.equal(Router.bootDestination({ hash: '', injectedPaired: true }), 'probe-paired');
  assert.equal(Router.bootDestination({ hash: '', injectedPaired: false }), 'entry');
});

test('#reset wins on an unpaired device instead of being painted over', () => {
  // The regression this exists for. `#reset` was handled at the top of the
  // script and `!paired → renderEntry()` at the bottom, hundreds of lines
  // apart, so an unpaired device with `#reset` drew the forgot-us screen and
  // then had the entry screen drawn over it. Two chains, one window.
  assert.equal(Router.bootDestination({ hash: '#reset', injectedPaired: false }), 'forgot-us');
});

test('#existing opens the join, paired or not', () => {
  assert.equal(Router.bootDestination({ hash: '#existing', injectedPaired: false }), 'existing');
  assert.equal(Router.bootDestination({ hash: '#existing', injectedPaired: true }), 'existing');
});

test('#setup pins the connect screen open on a paired device', () => {
  // For connecting a paired device to a second server, and the only way to
  // exercise the connect screen on a dev device without unpairing it.
  assert.equal(Router.bootDestination({ hash: '#setup', injectedPaired: true }), 'entry');
});

// ── the paired probe ────────────────────────────────────────────────────────

test('only an authed session opens the app', () => {
  // Anything less lands on the SPA's own legacy connect page — the "old path"
  // that must never appear on a phone. Seen live, twice, two different ways.
  assert.equal(Router.pairedProbe({ paired: true, session: 'authed' }), 'open');
  assert.equal(Router.pairedProbe({ paired: true, session: 'rejected' }), 'forgot-us');
  assert.equal(Router.pairedProbe({ paired: true, session: 'none' }), 'unreachable');
  assert.equal(Router.pairedProbe({ paired: true }), 'unreachable');
});

test('a probe that says unpaired goes to the entry screen, not the app', () => {
  assert.equal(Router.pairedProbe({ paired: false }), 'entry');
  assert.equal(Router.pairedProbe(null), 'entry');
});

// ── entries from Setup, and what a failed pairing says ──────────────────────

test('Setup opens the address form or one server\'s code screen', () => {
  assert.equal(Router.bootDestination({ hash: '#address', injectedPaired: false }), 'address');
  assert.equal(
    Router.bootDestination({ hash: '#pair?origin=http%3A%2F%2F192.168.1.20%3A8000&name=virtues', injectedPaired: false }),
    'code',
  );
  assert.deepEqual(
    Router.pairTarget('#pair?origin=http%3A%2F%2F192.168.1.20%3A8000&name=virtues&claimed=false'),
    { origin: 'http://192.168.1.20:8000', name: 'virtues', claimed: false },
  );
  assert.equal(Router.pairTarget('#pair?name=virtues'), null);
});

test('a line pasted from `virtues pair` fills both fields', () => {
  assert.deepEqual(Router.splitPasted('  http://192.168.1.20:8000   123 456 '), {
    address: 'http://192.168.1.20:8000',
    code: '123456',
  });
  assert.deepEqual(Router.splitPasted('http://[fd00::5]:8000 123456'), {
    address: 'http://[fd00::5]:8000',
    code: '123456',
  });
  // An address alone is left as typed.
  assert.equal(Router.splitPasted('http://192.168.1.20:8000'), null);
});

test('the server QR and a computer handoff are told apart', () => {
  // `virtues pair` encodes its own printed line, so the scanner reads it with
  // the same rule as a paste.
  assert.deepEqual(Router.splitPasted('http://192.168.1.20:8000   482 913'), {
    address: 'http://192.168.1.20:8000',
    code: '482913',
  });
  // A connected computer's handoff is JSON and must fall through to it.
  assert.equal(Router.splitPasted('{"v":1,"seed":"x","box_json":"{}"}'), null);
});

test('a pairing failure says which part failed, never the raw error', () => {
  const say = (raw) => Router.pairError(raw, { address: 'http://192.168.1.20:8000', device: 'this computer' });
  const dns = say('POST http://virtues.local:8000/api/pair/consume: error sending request: client error (Connect): dns error: failed to lookup address information: nodename nor servname provided, or not known');
  assert.match(dns.text, /couldn't find 192\.168\.1\.20:8000/);
  assert.doesNotMatch(dns.text, /dns error/);
  assert.match(say('client error (Connect): tcp connect error: Operation timed out (os error 60)').text, /directly/);
  assert.match(say('tcp connect error: Connection refused (os error 61)').text, /isn't serving on that port/);
  assert.match(say('pair failed: 401 Unauthorized — {"error":"invalid_or_expired_token"}').text, /run virtues pair for a new one/);
  assert.match(say('pair failed: 429 Too Many Requests — {"error":"too_many_attempts"}').text, /30 minutes/);
  // The server took the code; only this Mac's loopback is taken.
  const port = say('bind 127.0.0.1:7117: Address already in use (os error 48)');
  assert.equal(port.paired, true);
  assert.match(port.text, /Another copy of Virtues/);
  // Anything else still reads as a sentence.
  assert.match(say('something new').text, /^Pairing didn't finish\./);
});
