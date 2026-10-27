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
// Not "does the router work" — it is thirty lines of `if`. What these pin are
// the RULES, each of which was discovered by a box misbehaving on a bench and
// none of which is recoverable by reading the code:
//
//   · an advertisement's "I am online" byte goes stale and must be verified
//   · a claimed box never advertises, so BLE can only ever come up empty for it
//   · an unreachable box means an isolating LAN, not a broken box
//   · the launch hash and the injected paired flag must yield ONE destination
//
// A regression in any of these is a setup flow that strands someone, and none
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

const wantsWifi = (name = 'A', id = 'a') => ({ name, id, improvState: 2 });
const online = (name = 'A', id = 'a') => ({ name, id, improvState: 4 });
const server = (origin = 'http://10.0.0.5:8000') => ({ origin });

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
  // For setting up a SECOND box, and the only way to exercise setup on a dev
  // phone without unpairing it.
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

// ── discovery ───────────────────────────────────────────────────────────────

test('a box wanting wifi with nothing on the LAN is an unboxing', () => {
  const r = Router.afterDiscovery({ bleBoxes: [wantsWifi()], lanServers: [] });
  assert.equal(r.screen, 'phrase');
  assert.equal(r.box.improvState, 2);
});

test('a box on the LAN outranks a radio advertisement', () => {
  // Something answering on the network is better evidence than something
  // claiming over the air.
  const r = Router.afterDiscovery({ bleBoxes: [wantsWifi()], lanServers: [server()] });
  assert.equal(r.screen, 'pair');
});

test('BLE-visible but LAN-invisible means an isolating network, not a dead box', () => {
  // Offices. Pairing rides Bluetooth, so the LAN stops mattering.
  const r = Router.afterDiscovery({ bleBoxes: [online()], lanServers: [] });
  assert.equal(r.screen, 'route');
});

test('nothing anywhere keeps looking rather than dead-ending', () => {
  // A box just plugged in takes ~30s to advertise. The one-shot scan that used
  // to run here reliably missed it and dumped the owner on a screen that reads
  // as the wrong app entirely.
  assert.equal(
    Router.afterDiscovery({ bleBoxes: [], lanServers: [] }).screen, 'keep-looking');
});

test('the setup-AP breakglass outranks everything', () => {
  const r = Router.afterDiscovery({
    bleBoxes: [wantsWifi()], lanServers: [server()], provisionOpenServer: server('http://10.42.0.1:8000'),
  });
  assert.equal(r.screen, 'lan-provision');
  assert.equal(r.server.origin, 'http://10.42.0.1:8000');
});

// ── the stale advertisement ─────────────────────────────────────────────────

test('an "already online" claim is checked against the LAN, not believed', () => {
  // THE bug this rule exists for: the state byte is baked in when the box's BLE
  // service starts. A box that lost its network kept advertising "online" for
  // hours, and the screen told the owner to tap a chip that could not exist.
  const confirmed = Router.bleSetup({ bleBoxes: [online()], lanServers: [server()] });
  assert.equal(confirmed.screen, 'already-online');

  const lied = Router.bleSetup({ bleBoxes: [online()], lanServers: [] });
  assert.equal(lied.screen, 'route', 'an unconfirmed online claim must not dead-end');
});

test('the LAN is only consulted when every box claims to be online', () => {
  // The round trip is the whole cost of not believing the advertisement, so a
  // normal unboxing must not pay it.
  assert.equal(Router.needsLanCheck([wantsWifi()]), false);
  assert.equal(Router.needsLanCheck([]), false);
  assert.equal(Router.needsLanCheck([online()]), true);
  assert.equal(Router.needsLanCheck([online(), wantsWifi()]), false);
});

test('two unclaimed boxes ask rather than guess', () => {
  // A two-box household, or an office. Picking one silently would set up the
  // wrong box, and the owner would not find out until much later.
  const r = Router.bleSetup({ bleBoxes: [wantsWifi('A', 'a'), wantsWifi('B', 'b')], lanServers: [] });
  assert.equal(r.screen, 'choose');
  assert.equal(r.boxes.length, 2);
});

// ── the search loop ─────────────────────────────────────────────────────────

test('a claimed box can only ever reach the search loop over the LAN', () => {
  // It stops advertising the moment it is claimed, so Bluetooth will never see
  // it — which is why the loop peeks at the network each round.
  const r = Router.searchRound({ bleBoxes: [], lanServers: [server()] });
  assert.equal(r.screen, 'pair');
});

test('the search loop prefers a box that wants wifi over one that does not', () => {
  const r = Router.searchRound({ bleBoxes: [online('A', 'a'), wantsWifi('B', 'b')], lanServers: [] });
  assert.equal(r.screen, 'phrase');
  assert.equal(r.box.name, 'B');
});

// ── per-box step ────────────────────────────────────────────────────────────

test('a linked box goes straight to pairing', () => {
  assert.equal(Router.boxStep({ identity: { linked: true }, hasBle: true }), 'pair');
});

test('an unlinked box over Bluetooth claims a session before asking for the code', () => {
  // RPC 0x84 is session-gated. A box that was ALREADY online reached this step
  // with no session and sat forever on "getting your box's code…" while every
  // ask was refused — invisibly, since a refusal and "no code yet" are the same
  // empty answer.
  assert.equal(Router.boxStep({ identity: { linked: false }, hasBle: true }), 'phrase-then-link');
});

test('an unreachable box falls to the link step, which survives a hostile LAN', () => {
  // `identity` is null when the box could not be reached — the isolating office
  // network this whole flow exists for. Linking needs only the box's own
  // outbound internet, and it can always be skipped.
  assert.equal(Router.boxStep({ identity: null, hasBle: false }), 'link');
});

// ── naming ──────────────────────────────────────────────────────────────────

test('a box keeps its discovered name when it reports none', () => {
  assert.equal(Router.boxLabel({ identity: null, fallback: 'Mossy Godwit' }), 'Mossy Godwit');
  assert.equal(
    Router.boxLabel({ identity: { name: 'x', label: 'Quaint Tern' }, fallback: 'Mossy Godwit' }),
    'Quaint Tern');
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

test('a pairing failure says which part failed, never the raw error', () => {
  const say = (raw) => Router.pairError(raw, { address: 'http://192.168.1.20:8000', device: 'this computer' });
  const dns = say('POST http://virtues.local:8000/api/pair/consume: error sending request: client error (Connect): dns error: failed to lookup address information: nodename nor servname provided, or not known');
  assert.match(dns.text, /couldn't find 192\.168\.1\.20:8000/);
  assert.doesNotMatch(dns.text, /dns error/);
  assert.match(say('client error (Connect): tcp connect error: Operation timed out (os error 60)').text, /directly/);
  assert.match(say('tcp connect error: Connection refused (os error 61)').text, /isn't serving on that port/);
  assert.match(say('pair failed: 401 Unauthorized — {"error":"invalid_or_expired_token"}').text, /fresh one/);
  assert.match(say('pair failed: 429 Too Many Requests — {"error":"too_many_attempts"}').text, /30 minutes/);
  // The server took the code; only this Mac's loopback is taken.
  const port = say('bind 127.0.0.1:7117: Address already in use (os error 48)');
  assert.equal(port.paired, true);
  assert.match(port.text, /Another copy of Virtues/);
  // Anything else still reads as a sentence.
  assert.match(say('something new').text, /^Pairing didn't finish\./);
});
