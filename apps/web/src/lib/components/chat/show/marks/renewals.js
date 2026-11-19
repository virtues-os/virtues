// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/renewals.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* renewals.js: the lollipop timeline. What is coming up in a window of days, each item a dot on its date with a stem
   up to its name and amount, drawn in the dot grammar of the kept tiles.

   mark "lollipop". A chart mark for dated items with names, with amounts where they have them: payments and
   renewals, deadlines, deliveries.
   - Three columns, found by name: due (a date), label (the item, in a few words) and amount (in dollars, optional).
     Up to 40 rows. When a name is missing the column in that position is read instead. A row whose due is not a
     date is left out. A row with no amount is drawn with its name alone.
   - days, optional on the result, is the window's length: 7 to 92 days, 30 when it is left out, the first day
     included. The window starts on the result's as_of, the day the query ran, so a saved chat keeps its window;
     without as_of it starts on the first due date. A due date past the window widens the axis to reach it.
   - The axis is quiet: a hairline with a pinprick for each day, a slightly larger one on each Monday, and the date
     under the first day and under each Monday, any that would touch the one before left out.
   - Each item is a dot in the primary on its due day. Items on one day stack upward from the axis. Each dot carries
     a stem of pinpricks up to its label: the name in ink and the amount in muted ink. Labels sit right of their
     stem, or left of it near the right edge, in the lowest lane where they touch no other label and no other stem,
     so labels stack when days crowd. A label with no room left is not drawn until you rest on its dot.
   - There are no countdowns, no "due soon" and no color for size. Every dot is the same size; the amount is said in
     words. An item whose date has passed is the query's to leave out, never drawn as late.
   - The readout says the window and the total of the amounts in it, or the item you rest on: "Tue, Dec 1  Rent
     $1,450", or the date and the name alone for an item with no amount.
   - Rest on a dot or its label, or tab to the chart and use the arrow keys: the others dim and a ring grows on the
     dot. Home and End jump to the ends and Escape clears it.
   The left edge is a rule of fine dots with an ink dot at each end. In the frame each
   item's dot grows as the frame's sweep passes it, its stem rises and its label fades in.

   Contract: see the top of kit.js. Block: readout 26 + plot 200 + foot 26 = 252px. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'renewals', kinds: ['chart'], marks: ['lollipop'] });

const HEIGHT = kit.PLOT.height;
const PAD = Object.freeze({ top: 8, right: 10, bottom: 22, left: 4 });
const INSET = 10;            /* the first day sits this far right of the left rule */
const DAYS = Object.freeze({ min: 7, max: 92, dflt: 30 });   /* the window's length in days, the first one included */
const MAX_ROWS = 40;
const END_R = 1.75;
const DOT_R = 3.2;
const STACK = 7.5;           /* items on one day stack this far apart */
const LANE0 = 24, LANE = 15; /* the lowest label's middle above the axis, and the step between lanes */
const GAP = 5;               /* from a stem to its label */
const PIN_STEP = 4, PIN_R = 0.7;
const FONT = 12;
const GROW = 0.45, GROW_BOUNCE = 0.12;
const TAU = Math.PI * 2;
const DAY_MS = 864e5;
const MINUS = '−';

const CSS = '.vk-renewals .vk-readout.vk-tight{gap:0 10px}';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

/* ---------- money and dates ---------- */
function grouped(digits) {
  try { return new Intl.NumberFormat('en-US', { minimumFractionDigits: digits, maximumFractionDigits: digits, useGrouping: true }); } catch (err) { return null; }
}
const NF0 = grouped(0), NF2 = grouped(2);
/* $45, $15.49, $1,450: a dollar sign, thousands separators, and cents when there are any. */
function money(n) {
  const a = Math.abs(n);
  const cents = Math.round(a * 100) % 100 !== 0;
  const s = cents ? (NF2 ? NF2.format(a) : a.toFixed(2)) : (NF0 ? NF0.format(Math.round(a)) : String(Math.round(a)));
  return `${n < 0 ? MINUS : ''}$${s}`;
}
function formatter(o) {
  try { return new Intl.DateTimeFormat(undefined, o); } catch (err) { return null; }
}
const F_SHORT = formatter({ month: 'short', day: 'numeric' });
const F_DAY = formatter({ weekday: 'short', month: 'short', day: 'numeric' });
const fmt = (f, t) => (f ? f.format(t) : kit.formatTime(t, { withDate: true, withTime: false }));

/* Days are counted from Monday, January 3, 2000, so (index mod 7) is the weekday with Monday 0. A local date's
   midnight is built from its fields, so a day with a clock change is still one day. */
const EPOCH = new Date(2000, 0, 3).getTime();
const dayIndex = (t) => Math.round((new Date(t.getFullYear(), t.getMonth(), t.getDate()).getTime() - EPOCH) / DAY_MS);
const dateOf = (d) => new Date(2000, 0, 3 + d);
const isMonday = (d) => ((d % 7) + 7) % 7 === 0;

/* ---------- the model ---------- */
/* The window's length: the result's days, a whole number from 7 to 92, or 30. */
function spanOf(out) {
  const v = out && out.days;
  const n = isNum(v) ? v : typeof v === 'string' && /^\s*\d{1,3}\s*$/.test(v) ? Number(v) : NaN;
  return Number.isFinite(n) ? kit.clamp(Math.round(n), DAYS.min, DAYS.max) : DAYS.dflt;
}

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = (Array.isArray(out && out.rows) ? out.rows : []).slice(0, MAX_ROWS);
  const pick = (name, i) => (columns.includes(name) ? name : columns[i]);
  const dueKey = pick('due', 0), labelKey = pick('label', 1), amountKey = pick('amount', 2);
  const items = [];
  rows.forEach((r, i) => {
    const t = kit.parseTime(r[dueKey]);
    if (!t) return;
    const label = String(kit.formatCell(r[labelKey]) || '').trim() || 'Unnamed';
    items.push({ i, row: r, day: dayIndex(t), label, amount: isNum(r[amountKey]) ? r[amountKey] : null });
  });
  items.sort((a, b) => a.day - b.day || a.i - b.i);
  let prev = null, j = 0;
  for (const it of items) { j = prev === it.day ? j + 1 : 0; it.stack = j; prev = it.day; }
  const asOf = kit.parseTime(out && out.as_of);
  let start = asOf ? dayIndex(asOf) : items.length ? items[0].day : 0;
  if (items.length && items[0].day < start) start = items[0].day;
  const days = spanOf(out);
  let end = start + days - 1;
  if (items.length && items[items.length - 1].day > end) end = items[items.length - 1].day;
  let total = 0, priced = 0;
  for (const it of items) if (it.amount !== null) { total += it.amount; priced += 1; }
  return { columns, items, start, end, days, total: priced ? total : null, allPriced: priced === items.length };
}

const listText = (parts) => (parts.length > 1 ? `${parts.slice(0, -1).join(', ')} and ${parts[parts.length - 1]}` : parts[0] || '');

function ariaLabel(M) {
  if (!M.items.length) return 'Nothing coming up in the rows';
  const each = M.items.map((it) => `${it.label}${it.amount === null ? '' : ` ${money(it.amount)}`} on ${fmt(F_SHORT, dateOf(it.day))}`);
  let label = `Coming up from ${fmt(F_SHORT, dateOf(M.start))} to ${fmt(F_SHORT, dateOf(M.end))}: ${listText(each)}`;
  if (M.total !== null) label += M.allPriced ? `. ${money(M.total)} in all` : `. ${money(M.total)} in all for the items with amounts`;
  return `${label}.`;
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

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('renewals', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-renewals');
  el.appendChild(root);
  const blk = kit.block(root, {
    plotHeight: HEIGHT, readout: true, foot: opts.foot !== false,
    hint: 'Use the left and right arrow keys to read each item.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  blk.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let version = 0;
  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  /* The reveal: 'none' draws everything at once; 'live' grows each item as the frame's sweep passes it. */
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  let placedKey = '', placed = [];
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const B = () => box(width);
  const X = (day) => {
    const b = B(), x0 = b.left + INSET;
    return x0 + ((day - M.start) / Math.max(1, M.end - M.start)) * (b.right - x0);
  };
  const dotY = (it) => B().bottom - it.stack * STACK;
  const laneY = (k) => B().bottom - LANE0 - k * LANE;

  function growth(j) {
    if (mode === 'none') return 1;
    if (born[j] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[j], GROW, GROW_BOUNCE));
  }

  /* Each label's lane and side at this width. A label that reaches over another day's dot has to sit above that
     dot's label, or the dot's stem would rise through it. So each label's depth is the longest run of dots under
     it, one reaching over the next, and labels are placed from the shallowest up: a crowded stretch stacks as a
     staircase whose stems pass beside the labels below them, never through them.
     Each label takes the lowest lane where it touches no other label and no stem or stack of dots in that lane, and
     where its own stem touches no label below it. On a crowded day the lower dot takes the lower label. A label
     with no lane left is not drawn; resting on its dot shows it over the dot.
     The sides: every label starts right of its stem, or left of it where it would run past the right edge. Then
     each label in turn tries the other side and keeps it when that draws more labels, or as many with a shorter
     longest run, or with less depth in all, until every label is drawn or three passes change nothing. This runs
     once per width, theme and result. */
  function place(ctx) {
    const key = `${width}|${tokens.sans}|${version}`;
    if (key === placedKey) return placed;
    const b = B();
    const n = M.items.length;
    const lanes = Math.max(1, Math.floor((b.bottom - LANE0 - b.top - 7) / LANE) + 1);
    const hits = (list, a, z) => list.some(([p, q]) => a < q && z > p);
    ctx.save();
    ctx.font = `${FONT}px ${tokens.sans}`;
    const base = M.items.map((it) => {
      const x = X(it.day);
      const amount = it.amount === null ? '' : money(it.amount);
      const wn = ctx.measureText(it.label).width;
      const w = wn + (amount ? 5 + ctx.measureText(amount).width : 0);
      return { x, wn, w, amount, stack: it.stack };
    });
    ctx.restore();
    const textAt = (p, side) => { const t0 = side === 'right' ? p.x + GAP : p.x - GAP - p.w; return [t0, t0 + p.w]; };
    const fits = (p, side) => { const [t0, t1] = textAt(p, side); return t0 >= 1 && t1 <= width - 1; };
    /* a day's stack of dots is in the way of a label in any lane it reaches */
    const tops = new Map();
    for (const p of base) tops.set(p.x, Math.max(tops.get(p.x) ?? 0, p.stack));
    const piles = Array.from({ length: lanes }, () => []);
    for (const [x, top] of tops) {
      const yTop = b.bottom - top * STACK - DOT_R - 3;
      for (let k = 0; k < lanes; k++) if (laneY(k) + 8 > yTop) piles[k].push([x - DOT_R - 1.5, x + DOT_R + 1.5]);
    }

    /* Lays the labels out for one choice of sides: depths, then lanes. */
    function lay(sides) {
      const covers = base.map((p, i) => {
        const [t0, t1] = textAt(p, sides[i]);
        const under = [];
        base.forEach((q, j) => { if (j !== i && Math.abs(q.x - p.x) > 0.5 && q.x > t0 - 3.5 && q.x < t1 + 3.5) under.push(j); });
        return under;
      });
      const depth = new Array(n).fill(-1), visiting = new Array(n).fill(false);
      const deep = (i) => {
        if (depth[i] >= 0) return depth[i];
        if (visiting[i]) return 0;
        visiting[i] = true;
        let d = 0;
        for (const j of covers[i]) d = Math.max(d, deep(j) + 1);
        visiting[i] = false;
        depth[i] = d;
        return d;
      };
      let longest = 0, total = 0;
      for (let i = 0; i < n; i++) { const d = deep(i); longest = Math.max(longest, d); total += d; }
      const boxes = Array.from({ length: lanes }, () => []);   /* each label's text, 5px either side, kept apart */
      const inks = Array.from({ length: lanes }, () => []);    /* each label's text, 2px either side, no stem crosses */
      const stems = piles.map((list) => list.slice());
      const lane = new Array(n).fill(-1), side = sides.slice();
      const put = (j, sd) => {
        const p = base[j];
        if (!fits(p, sd)) return false;
        const [t0, t1] = textAt(p, sd);
        for (let k = 0; k < lanes; k++) {
          if (k > 0 && hits(inks[k - 1], p.x - 1.5, p.x + 1.5)) return false;
          if (hits(boxes[k], t0 - 5, t1 + 5) || hits(stems[k], t0 - 3, t1 + 3)) continue;
          boxes[k].push([t0 - 5, t1 + 5]);
          inks[k].push([t0 - 2, t1 + 2]);
          for (let l = 0; l < k; l++) stems[l].push([p.x - 1.5, p.x + 1.5]);
          lane[j] = k; side[j] = sd;
          return true;
        }
        return false;
      };
      const order = base.map((_, j) => j);
      order.sort((i, j) => depth[i] - depth[j] || base[i].x - base[j].x || base[i].stack - base[j].stack);
      const missed = [];
      for (const j of order) if (!put(j, sides[j])) missed.push(j);
      for (const j of missed) put(j, sides[j] === 'right' ? 'left' : 'right');
      const drawn = lane.filter((k) => k >= 0).length;
      return { lane, side, score: [-drawn, longest, total] };
    }

    const better = (a, c) => { for (let k = 0; k < a.length; k++) if (a[k] !== c[k]) return a[k] < c[k]; return false; };
    const sides = base.map((p) => (fits(p, 'right') || !fits(p, 'left') ? 'right' : 'left'));
    let best = lay(sides);
    for (let pass = 0; pass < 3 && best.score[0] > -n; pass++) {
      let changed = false;
      for (let i = 0; i < n; i++) {
        const was = sides[i], other = was === 'right' ? 'left' : 'right';
        if (!fits(base[i], other)) continue;
        sides[i] = other;
        const next = lay(sides);
        if (better(next.score, best.score)) { best = next; changed = true; } else sides[i] = was;
      }
      if (!changed || best.score[0] === -n) break;
    }
    placed = base.map((p, j) => {
      const k = best.lane[j];
      if (k < 0) return { ...p, lane: -1, side: 'right', tx: p.x, rect: null };
      const [t0] = textAt(p, best.side[j]);
      return { ...p, lane: k, side: best.side[j], tx: t0, rect: [t0 - 3, laneY(k) - 9, p.w + 6, 18] };
    });
    placedKey = key;
    return placed;
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
    if (!width || !M.items.length) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const T = tokens, b = B();
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const P = place(ctx);

    /* the axis: a hairline, a pinprick each day and a larger one each Monday, arriving behind the sweep */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(L, floor, R - L, 1);
    const pins = new Path2D(), mondays = new Path2D();
    for (let d = M.start; d <= M.end; d++) {
      const x = X(d);
      if (isMonday(d)) { mondays.moveTo(x + 1.1, floor + 5.5); mondays.arc(x, floor + 5.5, 1.1, 0, TAU); }
      else { pins.moveTo(x + 0.6, floor + 5.5); pins.arc(x, floor + 5.5, 0.6, 0, TAU); }
    }
    ctx.fillStyle = kit.rgba(T.rgb.muted, T.rgb.muted[3] * 0.55);
    ctx.fill(pins);
    ctx.fillStyle = T.muted;
    ctx.fill(mondays);
    ctx.restore();

    /* dates under the first day and each Monday, any that would touch the one before left out */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textBaseline = 'alphabetic';
    let lastEnd = -Infinity;
    for (let d = M.start; d <= M.end; d++) {
      if (d !== M.start && !isMonday(d)) continue;
      const x = X(d), text = fmt(F_SHORT, dateOf(d)), w = ctx.measureText(text).width;
      const first = d === M.start;
      let x0 = first ? x - 2 : x - w / 2;
      if (x0 + w > width - 2) x0 = width - 2 - w;
      if (x0 < lastEnd + 8) continue;
      lastEnd = x0 + w;
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - x, 0, 24) : 1);
      if (a <= 0) continue;
      ctx.globalAlpha = a;
      ctx.textAlign = 'left';
      ctx.fillText(text, x0, HEIGHT - 5);
    }
    ctx.restore();

    /* the dotted rule at the left edge, with an ink dot at each end */
    dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); disc(ctx, L, top, END_R); disc(ctx, L, floor, END_R); ctx.fill();

    const hv = hover;
    const dimA = hv !== null ? kit.lerp(1, 0.4, dim.x) : 1;
    const alphaOf = (j) => (hv !== null && hv !== j ? dimA : 1);

    /* the stems, each rising from its dot to its label as it grows */
    const muted = T.rgb.muted;
    M.items.forEach((it, j) => {
      const p = P[j], g = growth(j);
      if (p.lane < 0 || g <= 0) return;
      const y0 = dotY(it) - DOT_R - 2.5, y1 = laneY(p.lane) + 1;
      const yTop = y0 + (y1 - y0) * kit.clamp(g, 0, 1);
      ctx.fillStyle = kit.rgba(muted, muted[3] * 0.7 * alphaOf(j));
      ctx.beginPath();
      for (let y = y0; y >= yTop - 0.01; y -= PIN_STEP) disc(ctx, p.x, y, PIN_R);
      ctx.fill();
    });

    /* the labels: the name in ink, the amount in muted ink */
    ctx.save();
    ctx.font = `${FONT}px ${T.sans}`;
    ctx.textBaseline = 'middle';
    ctx.textAlign = 'left';
    M.items.forEach((it, j) => {
      const p = P[j], g = growth(j);
      if (p.lane < 0 || g <= 0) return;
      const a = kit.clamp((g - 0.5) / 0.5, 0, 1) * alphaOf(j);
      if (a <= 0) return;
      const y = laneY(p.lane);
      ctx.globalAlpha = a;
      ctx.fillStyle = T.ink;
      ctx.fillText(it.label, p.tx, y);
      if (p.amount) { ctx.fillStyle = hv === j ? T.ink : T.muted; ctx.fillText(p.amount, p.tx + p.wn + 5, y); }
    });
    ctx.restore();

    /* the dots, in the primary, stacked on a crowded day */
    const s0 = T.rgb.s0;
    M.items.forEach((it, j) => {
      const g = growth(j);
      if (g <= 0) return;
      const x = P[j].x, y = dotY(it);
      const r = DOT_R * kit.clamp(g, 0, 1.2) + (hv === j ? 0.5 : 0);
      ctx.fillStyle = T.page;
      ctx.beginPath(); disc(ctx, x, y, r + 1.2); ctx.fill();
      ctx.fillStyle = kit.rgba(s0, s0[3] * alphaOf(j));
      ctx.beginPath(); disc(ctx, x, y, r); ctx.fill();
    });

    /* the ring on the item you rest on, and its label over its dot when it had no room of its own */
    if (hv !== null && M.items[hv] && growth(hv) > 0) {
      const it = M.items[hv], p = P[hv], y = dotY(it);
      ctx.save();
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(s0, Math.min(1, s0[3] * 1.1) * 0.9);
      ctx.beginPath(); ctx.arc(p.x, y, DOT_R + 1.5 + 3.5 * ring.x, 0, TAU); ctx.stroke();
      if (p.lane < 0) {
        const tx = kit.clamp(p.x - p.w / 2, 2, width - 2 - p.w), ty = y - DOT_R - 14;
        ctx.font = `${FONT}px ${T.sans}`;
        ctx.textBaseline = 'middle';
        ctx.textAlign = 'left';
        ctx.globalAlpha = reduced() ? 1 : kit.clamp(ring.x * 1.6, 0, 1);
        ctx.fillStyle = T.page;
        ctx.fillRect(tx - 4, ty - 9, p.w + 8, 18);
        ctx.fillStyle = T.ink;
        ctx.fillText(it.label, tx, ty);
        if (p.amount) ctx.fillText(p.amount, tx + p.wn + 5, ty);
      }
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    const node = blk.readout;
    if (!node || !M.items.length) return;
    if (hover === null || !M.items[hover]) {
      const range = { when: `${fmt(F_SHORT, dateOf(M.start))} to ${fmt(F_SHORT, dateOf(M.end))}` };
      fitReadout(node, [M.total === null ? [range] : [range, { label: 'Total', value: money(M.total) }]]);
      return;
    }
    const it = M.items[hover];
    const what = it.amount === null ? { label: it.label } : { label: it.label, value: money(it.amount) };
    const when = dateOf(it.day);
    fitReadout(node, [
      [{ when: fmt(F_DAY, when) }, what],
      [{ when: fmt(F_SHORT, when) }, what],
    ]);
  }

  function setHover(j, from) {
    if (j === hover) { hoverFrom = from; return; }
    hover = j; hoverFrom = from;
    if (j !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows, the others dim, or items grow ---------- */
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
    M.items.forEach((it, j) => {
      if (born[j] === undefined && (reveal === Infinity || X(it.day) <= reveal)) born[j] = clock;
    });
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
  /* A label you point at reads its item; elsewhere the nearest day does, and on a crowded day the dot nearest the
     pointer. */
  function nearest(px, py) {
    const P = placed.length === M.items.length ? placed : null;
    if (P) {
      for (let j = 0; j < P.length; j++) {
        const r = P[j].rect;
        if (r && px >= r[0] && px <= r[0] + r[2] && py >= r[1] && py <= r[1] + r[3]) return j;
      }
    }
    let best = 0, dist = Infinity;
    M.items.forEach((it, j) => {
      const d = Math.abs(X(it.day) - px) * 4 + Math.abs(dotY(it) - py) * 0.25;
      if (d < dist) { best = j; dist = d; }
    });
    return best;
  }
  const pick = (e) => { const r = canvas.getBoundingClientRect(); return nearest(e.clientX - r.left, e.clientY - r.top); };
  const onMove = (e) => {
    if (!width || !M.items.length) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.items.length) return;
    const j = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === j) { setHover(null, null); return; }
    setHover(j, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  blk.plot.addEventListener('pointermove', onMove);
  blk.plot.addEventListener('pointerdown', onDown);
  blk.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(blk.plot, {
    count: () => M.items.length,
    get: () => hover,
    page: 5,
    set: (j, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(j, 'key');
    },
  });

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    version += 1;
    born = [];
    hover = null; hoverFrom = null;
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
      root.remove();
    },
  };
}
