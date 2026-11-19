// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/moments.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* moments.js: Swimlane event plot. kind "chart", mark "swimlanes".

   One named lane for each group, with a dot at the real time of each event in it, so the timing of events shows
   across groups. It fits irregular events over weeks or months where the timing matters more than any total: what
   happened in each thread, activities by kind, purchases by category. A quiet stretch stays plain track, and
   nothing is counted.

   - Three columns, found by name: lane (the group's name; a column named thread or project works too), at (a time,
     or a date) and what (a few words for the event). When a name is missing, the column in that position is read
     instead: the first for lane, the second for at, the third for what. Up to 400 rows and 12 lanes. A row whose at
     is not a time is left out, and rows past the twelfth lane are left out. A row with no lane name goes in a lane
     called "A lane without a name".
   - One lane per group, in the order the groups first appear in the rows, so the query's order is the order the
     person sees. Each lane is its name in ink over a quiet track, and each event is a dot in the primary at its time
     on the track.
   - Time is real time across the window, from the day of the earliest event to the result's as_of, the day the
     query ran, or to the latest event when there is no as_of. Mondays are marked on the axis under the lanes. The
     chart draws no grid of days, counts nothing, and leaves a stretch with no events as plain track: nothing marks
     a gap as missed and nothing marks a busy stretch as good.
   - Dots that would touch in one lane step a little above or below the track, so each one can be seen and read.
   - Rest on a dot, or tab to the chart and use the arrow keys: left and right move through a lane's events in time,
     up and down move to the nearest event in the lane above or below, Home and End jump to the first and last, and
     Escape clears it. A ring grows on the dot, its words appear beside it on the track, the other lanes dim, and the
     readout says when, the lane and what happened: "Tue, Nov 10, 7:00 PM  Learning Portuguese  Lesson".
   Height: the lanes set it. Before the result arrives the block is drawn for four lanes; a result with another
   number settles it once, as the rows land. Lanes are 38px apart up to six and 32px up to twelve.
   Block: readout 26 + plot (184 for four lanes) + foot 26 = 236px.

   Contract: see the top of kit.js. While a query runs past 300 ms the frame's dot runs along the time axis (box.bottom).
   When the rows arrive the dots grow as the frame's sweep passes them, through setReveal. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'moments', kinds: ['chart'], marks: ['swimlanes'] });

const MAX_ROWS = 400;
const MAX_LANES = 12;
const NOMINAL = 4;             /* the lanes a result is drawn for before it says how many */
const TOP = 4;
const SPINE_X = 4;             /* the spine at the left edge */
const INDENT = 14;             /* names and time start here, clear of the spine */
const RIGHT = 10;
const AXIS_GAP = 18;           /* from the last track down to the time axis */
const AXIS_FOOT = 22;          /* under the axis, for its dates */
const TIERS = Object.freeze([
  Object.freeze({ upTo: 6, pitch: 38, name: 14, track: 28 }),
  Object.freeze({ upTo: MAX_LANES, pitch: 32, name: 13, track: 24 }),
]);
const DOT_R = 3.1;
const DODGE = Object.freeze([0, -3.6, 3.6]);
const END_R = 1.75;
const GROW = 0.45, GROW_BOUNCE = 0.12;
const TAU = Math.PI * 2;
const DAY_MS = 864e5;
const MIN_SPAN_DAYS = 7;

const CSS = '.vk-moments .vk-readout.vk-tight{gap:0 10px}';

const tierFor = (n) => TIERS.find((t) => n <= t.upTo) || TIERS[TIERS.length - 1];
const LANE_NAMES = Object.freeze(['lane', 'thread', 'project']);
const UNNAMED = 'A lane without a name';
const laneKeyOf = (columns) => LANE_NAMES.find((n) => columns.includes(n)) || columns[0];
function lanesIn(input) {
  const rows = input && Array.isArray(input.rows) ? input.rows : null;
  if (!rows || !rows.length) return NOMINAL;
  const key = laneKeyOf(Array.isArray(input.columns) ? input.columns : []);
  const seen = new Set();
  for (const r of rows) { seen.add(String(r[key] ?? '')); if (seen.size >= MAX_LANES) break; }
  return kit.clamp(seen.size, 1, MAX_LANES);
}
const trackY = (k, t) => TOP + k * t.pitch + t.track;
const axisY = (n) => trackY(n - 1, tierFor(n)) + AXIS_GAP;
const plotHeight = (n) => axisY(n) + AXIS_FOOT;

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + plotHeight(lanesIn(input)) + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width, input) {
  const n = lanesIn(input), t = tierFor(n);
  return { left: SPINE_X, right: Math.max(SPINE_X + 1, width - RIGHT), top: trackY(0, t), bottom: axisY(n), width, height: plotHeight(n) };
}

/* ---------- words ---------- */
function formatter(o) {
  try { return new Intl.DateTimeFormat(undefined, o); } catch (err) { return null; }
}
const F_SHORT = formatter({ month: 'short', day: 'numeric' });
const F_DAY = formatter({ weekday: 'short', month: 'short', day: 'numeric' });
const F_TIME = formatter({ hour: 'numeric', minute: '2-digit' });
const fmt = (f, t, fallback) => (f ? f.format(t) : fallback(t));
const shortDate = (t) => fmt(F_SHORT, t, (x) => kit.formatTime(x, { withDate: true, withTime: false }));
function whenOf(m) {
  const day = fmt(F_DAY, m.t, (x) => kit.formatTime(x, { withDate: true, withTime: false }));
  return m.dateOnly ? day : `${day}, ${fmt(F_TIME, m.t, (x) => kit.formatTime(x))}`;
}
const listText = (parts) => (parts.length > 1 ? `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1]}` : parts[0] || '');

/* ---------- the model ---------- */
const DATE_ONLY = /^\d{4}-\d{2}-\d{2}$/;
const startOfDay = (t) => new Date(t.getFullYear(), t.getMonth(), t.getDate());
const endOfDay = (t) => new Date(t.getFullYear(), t.getMonth(), t.getDate() + 1);

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = (Array.isArray(out && out.rows) ? out.rows : []).slice(0, MAX_ROWS);
  const pick = (name, i) => (columns.includes(name) ? name : columns[i]);
  const laneKey = laneKeyOf(columns), atKey = pick('at', 1), whatKey = pick('what', 2);
  const lanes = [];
  const index = new Map();
  rows.forEach((r, i) => {
    const t = kit.parseTime(r[atKey]);
    if (!t) return;
    const name = String(r[laneKey] ?? '').trim() || UNNAMED;
    let k = index.get(name);
    if (k === undefined) {
      if (lanes.length >= MAX_LANES) return;
      k = lanes.length;
      index.set(name, k);
      lanes.push({ name, moments: [] });
    }
    const what = String(kit.formatCell(r[whatKey]) || '').trim();
    lanes[k].moments.push({ i, row: r, k, t, ms: t.getTime(), what, dateOnly: typeof r[atKey] === 'string' && DATE_ONLY.test(r[atKey]) });
  });
  for (const p of lanes) p.moments.sort((a, b) => a.ms - b.ms || a.i - b.i);
  /* reading order: lane by lane, each in time */
  const order = lanes.flatMap((p) => p.moments);
  order.forEach((m, j) => { m.j = j; });
  let lo = Infinity, hi = -Infinity;
  for (const m of order) { lo = Math.min(lo, m.ms); hi = Math.max(hi, m.ms); }
  if (!order.length) { lo = hi = Date.now(); }
  const asOf = kit.parseTime(out && out.as_of);
  let start = startOfDay(new Date(lo)).getTime();
  let end = endOfDay(new Date(asOf && asOf.getTime() >= hi ? asOf.getTime() : hi)).getTime();
  if (end - start < MIN_SPAN_DAYS * DAY_MS) start = end - MIN_SPAN_DAYS * DAY_MS;
  const n = Math.max(1, lanes.length);
  return { columns, lanes, order, start, end, n, tier: tierFor(n) };
}

function ariaLabel(M) {
  if (!M.order.length) return 'Swimlane plot of events, with no events to draw.';
  const names = listText(M.lanes.map((p) => p.name));
  const last = new Date(M.end - 1);
  return `Swimlane plot of events, one lane each for ${names}, each event set on its date, from ${shortDate(new Date(M.start))} to ${shortDate(last)}.`;
}

/* Writes the first of `levels` that fits the readout's line; the last one is written whatever its width. */
const fits = (node) => node.scrollWidth <= node.clientWidth + 1
  && [...node.querySelectorAll('.vk-lab')].every((e) => e.scrollWidth <= e.clientWidth + 1);
function fitReadout(node, levels) {
  if (!node) return;
  for (let l = 0; l < levels.length; l++) {
    node.classList.toggle('vk-tight', l > 0);
    kit.setReadout(node, levels[l]);
    if (!node.clientWidth || l === levels.length - 1 || fits(node)) return;
  }
}

/* Cuts text to `room` px with an ellipsis. */
function cut(ctx, text, room) {
  if (ctx.measureText(text).width <= room) return text;
  let s = text;
  while (s.length > 1 && ctx.measureText(`${s}…`).width > room) s = s.slice(0, -1);
  return `${s.trimEnd()}…`;
}

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('moments', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-moments');
  el.appendChild(root);
  let out = output || {};
  let M = model(out);
  const blk = kit.block(root, {
    plotHeight: plotHeight(lanesIn(out)), readout: true, foot: opts.foot !== false,
    hint: 'Use the left and right arrow keys to read the events in a lane, and up and down to move between lanes.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  blk.plot.appendChild(canvas);

  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  /* The reveal: 'none' draws everything at once; 'live' grows each event as the frame's sweep passes it. */
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  let dodgeKey = '', dodge = [];
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const H = () => plotHeight(M.n);
  const B = () => box(width, out);
  const x0 = () => SPINE_X + INDENT;
  const X = (ms) => { const R = B().right; return x0() + ((ms - M.start) / Math.max(1, M.end - M.start)) * (R - x0()); };
  const rowTrack = (k) => trackY(k, M.tier);
  /* Each dot's step off the track: dots that would touch in one lane take the next free step. */
  function offsets() {
    const key = `${width}|${M.order.length}|${M.start}|${M.end}`;
    if (key === dodgeKey) return dodge;
    dodge = new Array(M.order.length).fill(0);
    for (const p of M.lanes) {
      const last = DODGE.map(() => -Infinity);
      for (const m of p.moments) {
        const x = X(m.ms);
        let s = DODGE.findIndex((_, d) => x - last[d] >= DOT_R * 2 + 0.6);
        if (s < 0) s = 0;
        last[s] = x;
        dodge[m.j] = DODGE[s];
      }
    }
    dodgeKey = key;
    return dodge;
  }
  const dotAt = (m) => [X(m.ms), rowTrack(m.k) + (offsets()[m.j] || 0)];

  function growth(j) {
    if (mode === 'none') return 1;
    if (born[j] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[j], GROW, GROW_BOUNCE));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (ctx, x, y, r) => { ctx.moveTo(x + r, y); ctx.arc(x, y, r, 0, TAU); };

  function draw() {
    if (!width || !M.order.length) return;
    const HH = H();
    const ctx = kit.fitCanvas(canvas, width, HH);
    const T = tokens, b = B();
    const L = b.left, R = b.right, floor = b.bottom, X0 = x0();
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const hv = hover;
    const hm = hv !== null ? M.order[hv] : null;
    const dimA = hv !== null ? kit.lerp(1, 0.4, dim.x) : 1;

    /* the names, each over its track */
    ctx.save();
    ctx.font = `13px ${T.sans}`;
    ctx.textBaseline = 'alphabetic';
    ctx.textAlign = 'left';
    ctx.globalAlpha = labelA;
    M.lanes.forEach((p, k) => {
      ctx.fillStyle = hm && hm.k !== k ? kit.rgba(T.rgb.ink, T.rgb.ink[3] * kit.lerp(1, 0.6, dim.x)) : T.ink;
      ctx.fillText(cut(ctx, p.name, R - X0), X0, TOP + k * M.tier.pitch + M.tier.name);
    });
    ctx.restore();

    /* the tracks and the time axis arrive behind the sweep */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HH); ctx.clip(); }
    M.lanes.forEach((p, k) => dotted(ctx, X0, rowTrack(k) + 0.5, R, rowTrack(k) + 0.5, 4, 1, T.border));
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(L, floor, R - L, 1);
    /* a pinprick on each Monday, under the axis */
    const mondays = new Path2D();
    const first = new Date(M.start);
    const mondaysAt = [];
    for (let d = new Date(first.getFullYear(), first.getMonth(), first.getDate()); d.getTime() <= M.end; d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1)) {
      if (d.getDay() !== 1) continue;
      const x = X(d.getTime());
      if (x < X0 - 0.5 || x > R + 0.5) continue;
      mondaysAt.push([x, d]);
      mondays.moveTo(x + 1.1, floor + 5.5); mondays.arc(x, floor + 5.5, 1.1, 0, TAU);
    }
    ctx.fillStyle = T.muted;
    ctx.fill(mondays);
    ctx.restore();

    /* dates: the first day, then each Monday, any that would touch the one before left out */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textBaseline = 'alphabetic';
    ctx.textAlign = 'left';
    let lastEnd = -Infinity;
    for (const [x, d, isFirst] of [[X0, first, true], ...mondaysAt]) {
      const text = shortDate(d), w = ctx.measureText(text).width;
      let a0 = isFirst ? x - 2 : x - w / 2;
      if (a0 + w > width - 2) a0 = width - 2 - w;
      if (a0 < lastEnd + 8) continue;
      lastEnd = a0 + w;
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - x, 0, 24) : 1);
      if (a <= 0) continue;
      ctx.globalAlpha = a;
      ctx.fillText(text, a0, HH - 5);
    }
    ctx.restore();

    /* the spine: fine dots from the first track to the axis, an ink dot at each end */
    dotted(ctx, L, floor, L, b.top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); disc(ctx, L, b.top, END_R); disc(ctx, L, floor, END_R); ctx.fill();

    /* the events: a dot in the primary for each, on page-colored halos so touching dots stay apart */
    const s0 = T.rgb.s0;
    const halos = new Path2D(), dots = [new Path2D(), new Path2D()];
    for (const m of M.order) {
      const g = growth(m.j);
      if (g <= 0) continue;
      const [x, y] = dotAt(m);
      const r = DOT_R * kit.clamp(g, 0, 1.2) + (m.j === hv ? 0.5 : 0);
      halos.moveTo(x + r + 1.1, y); halos.arc(x, y, r + 1.1, 0, TAU);
      const k = hm && hm.k !== m.k ? 1 : 0;
      dots[k].moveTo(x + r, y); dots[k].arc(x, y, r, 0, TAU);
    }
    ctx.fillStyle = T.page;
    ctx.fill(halos);
    ctx.fillStyle = T.s0;
    ctx.fill(dots[0]);
    ctx.fillStyle = kit.rgba(s0, s0[3] * dimA);
    ctx.fill(dots[1]);

    /* the event you rest on: a ring, and its words over it */
    if (hm && growth(hm.j) > 0) {
      const [x, y] = dotAt(hm);
      ctx.save();
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(s0, Math.min(1, s0[3] * 1.1) * 0.9);
      ctx.beginPath(); ctx.arc(x, y, DOT_R + 1.5 + 3.5 * ring.x, 0, TAU); ctx.stroke();
      if (hm.what) {
        /* beside the dot on its track, clear of the names above, on the page's own paint so the dots it passes
           over do not show through; left of the dot where it would run past the right edge */
        ctx.font = `12px ${T.sans}`;
        ctx.textBaseline = 'middle';
        ctx.textAlign = 'left';
        const off = DOT_R + 9;
        const text = cut(ctx, hm.what, Math.max(x - off - X0, R - x - off));
        const w = ctx.measureText(text).width;
        const tx = x + off + w <= R + 4 ? x + off : x - off - w;
        ctx.globalAlpha = reduced() ? 1 : kit.clamp(ring.x * 1.6, 0, 1);
        ctx.fillStyle = T.page;
        ctx.fillRect(tx - 4, y - 8, w + 8, 16);
        ctx.fillStyle = T.ink;
        ctx.fillText(text, tx, y);
      }
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    const node = blk.readout;
    if (!node || !M.order.length) return;
    if (hover === null || !M.order[hover]) {
      fitReadout(node, [[{ when: `${shortDate(new Date(M.start))} to ${shortDate(new Date(M.end - 1))}` }]]);
      return;
    }
    const m = M.order[hover], name = M.lanes[m.k].name;
    const what = m.what || '-';
    fitReadout(node, [
      [{ when: whenOf(m) }, { label: name, value: what }],
      [{ when: shortDate(m.t) }, { label: name, value: what }],
    ]);
  }

  function setHover(j, from) {
    if (j === hover) { hoverFrom = from; return; }
    hover = j; hoverFrom = from;
    if (j !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows, the others dim, or events grow ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    if (reduced()) { ring.x = 1; ring.v = 0; dim.x = hover !== null ? 1 : 0; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, hover !== null ? 1 : 0, dt, 0.2, 0);
      moving = ring.x !== 1 || dim.x !== (hover !== null ? 1 : 0);
    }
    if (mode === 'live' && !reduced()) {
      if (clock - revealAt < 0.35) moving = true;
      for (let j = 0; j < born.length; j++) if (born[j] !== undefined && clock - born[j] < 1.2) { moving = true; break; }
      if (reveal < Infinity) moving = true;
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    for (const m of M.order) if (born[m.j] === undefined && (reveal === Infinity || X(m.ms) <= reveal)) born[m.j] = clock;
  }
  function rowsWait(on) {
    for (const node of [blk.readout, blk.foot]) {
      if (!node) continue;
      node.classList.toggle('vk-fade', !on && !reduced());
      node.classList.toggle('vk-wait', on);
    }
  }
  function setReveal(x) {
    if (mode === 'none' && x === Infinity) return;
    if (mode === 'none') { mode = 'live'; revealAt = clock; }
    reveal = x;
    if (width) markBorn();
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  /* The lane under the pointer, then its event nearest in time. */
  function nearest(px, py) {
    let k = 0, dy = Infinity;
    M.lanes.forEach((p, i) => { const d = Math.abs(rowTrack(i) - 6 - py); if (d < dy) { dy = d; k = i; } });
    let best = null, dx = Infinity;
    for (const m of M.lanes[k].moments) { const d = Math.abs(X(m.ms) - px); if (d < dx) { dx = d; best = m.j; } }
    return best;
  }
  const pick = (e) => { const r = canvas.getBoundingClientRect(); return nearest(e.clientX - r.left, e.clientY - r.top); };
  /* A mouse reads the event under it and lets go when it leaves. A finger lifts as soon as it taps, so a tap keeps
     its event until the same one is tapped again or the chart loses focus. A drag along a lane scrubs. */
  const onMove = (e) => {
    if (!width || !M.order.length) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.order.length) return;
    const j = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === j) { setHover(null, null); return; }
    setHover(j, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  blk.plot.addEventListener('pointermove', onMove);
  blk.plot.addEventListener('pointerdown', onDown);
  blk.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(blk.plot, {
    count: () => M.order.length,
    get: () => hover,
    set: (j, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(j, 'key');
    },
  });
  /* Up and down: the event nearest in time in the lane above or below. */
  const onRowKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey || (e.key !== 'ArrowUp' && e.key !== 'ArrowDown')) return;
    if (!M.order.length) return;
    e.preventDefault();
    if (hover === null) { setHover(e.key === 'ArrowDown' ? 0 : M.lanes[M.lanes.length - 1].moments[0].j, 'key'); return; }
    const m = M.order[hover];
    const k = kit.clamp(m.k + (e.key === 'ArrowDown' ? 1 : -1), 0, M.lanes.length - 1);
    if (k === m.k) return;
    let best = null, d = Infinity;
    for (const q of M.lanes[k].moments) { const dd = Math.abs(q.ms - m.ms); if (dd < d) { d = dd; best = q.j; } }
    if (best !== null) setHover(best, 'key');
  };
  blk.plot.addEventListener('keydown', onRowKey);

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    dodgeKey = '';
    born = [];
    hover = null; hoverFrom = null;
    blk.plot.style.height = `${H()}px`;
    kit.setLabel(blk.plot, ariaLabel(M));
    readout();
    kit.foot(blk.foot, out, opts);
    if (mode === 'live') { if (width) markBorn(); rowsWait(reveal < Infinity); }
    draw();
  }
  const stopWidth = kit.observeWidth(blk.plot, (w) => {
    width = w;
    if (mode === 'live') markBorn();
    readout();
    draw();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; draw(); }
  });
  const stopDpr = kit.watchDpr(() => draw());
  const stopMotion = kit.watchReducedMotion(() => { rowsWait(mode === 'live' && reveal < Infinity); loop.wake(); });
  apply();
  if (mode === 'live') { revealAt = clock; loop.wake(); }

  return {
    plot: blk.plot,
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      out = next || {};
      apply();
    },
    setReveal,
    destroy() {
      loop.stop(); stopKeys(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      blk.plot.removeEventListener('pointermove', onMove);
      blk.plot.removeEventListener('pointerdown', onDown);
      blk.plot.removeEventListener('pointerleave', onLeave);
      blk.plot.removeEventListener('keydown', onRowKey);
      root.remove();
    },
  };
}
