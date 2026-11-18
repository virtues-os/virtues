// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/months.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* months.js: the cumulative comparison. This period's running total beside the previous period's, step by step,
   drawn in the dot grammar of the kept tiles, so the reader sees whether this period is running ahead of or behind
   the last one, and by how much.

   mark "cumulative". A chart mark for any measure that adds up: money spent this month against last month, steps
   this week against last week, hours today against the same day last week.
   - step_kind, optional on the result, says what a step is: "day" (the day of the month, 1 to 31, the default),
     "weekday" (the day of the week, 1 to 7, Monday first) or "hour" (the hour of the day, 0 to 23). Anything else
     is read as day.
   - unit, optional on the result: "USD" writes money, $2,512 with cents under $100, and an axis of $2,000 or $5k.
     Any other word, such as steps or km, writes plain numbers with grouping (41,200), puts the word after each
     figure in the readout and the figure's label, and writes it once on the axis, beside the top tick. With no
     unit the numbers are plain.
   - Three columns: the step, found by name as step (or day), else the first column; then the two running totals,
     this period's and the previous one's, in that order (this_month and last_month are still found by name). Each
     total runs up to and including its step. A total's column name is its label, so a query aliases it the way
     it should read (AS "This week"). Up to 31 rows; a second row for a step is ignored. A null is a step with no
     total: this period's is null after now, and the previous one's past the end of a shorter period.
   - This period is a line of dots in the primary, one dot for each step and fine dots between them, ending in a
     larger dot on the latest step it has. The previous period is a pencil line: a thin solid line in ink at 60%.
     The axis starts at zero. There is no target line, and nothing is colored for more or less.
   - The x axis runs from the first step to the last step in the rows, and holds at least 28 days, 7 weekdays or
     24 hours, so a period that has just begun is still drawn against the whole period. It is labelled 1, 5, 10 for
     days, Mon to Sun for weekdays, and 12 AM, 6 AM, 12 PM, 6 PM for hours.
   - Rest on a step, or tab to the chart and use the arrow keys: a dotted rule marks the step, a ring grows on each
     line's point, a hairline joins the two points, and the readout says both totals and the difference in plain
     words, as "By the 17th  This month $2,512  Last month $2,370  $142 more than last month" or "By Wednesday
     This week 41,200 steps  Last week 38,950 steps  2,250 more than last week". It never says better or worse. A
     narrow column keeps the figures and shortens the words.
   The rest of the grammar is series.js's: the y axis is a rule of fine dots with an ink dot at its top and its foot,
   and the grid is dotted. In the frame each step's point appears as the frame's sweep passes it.

   Contract: see the top of kit.js. Block: readout 26 + plot 200 + foot 26 = 252px. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'months', kinds: ['chart'], marks: ['cumulative'] });

const HEIGHT = kit.PLOT.height;
const PAD = Object.freeze({ top: 8, right: 10, bottom: 22, left: 44 });
const INSET = 8;             /* the first step sits this far right of the y axis */
/* What a step is: its range, the fewest steps the axis holds, and the word for one step and for the run of them. */
const STEPS = Object.freeze({
  day: Object.freeze({ lo: 1, hi: 31, span: 28, noun: 'day', of: 'day of the month' }),
  weekday: Object.freeze({ lo: 1, hi: 7, span: 7, noun: 'day', of: 'day of the week' }),
  hour: Object.freeze({ lo: 0, hi: 23, span: 24, noun: 'hour', of: 'hour of the day' }),
});
const WEEKDAYS = Object.freeze(['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday', 'Sunday']);
const UNIT_MAX = 12;         /* the longest unit word drawn */
const END_R = 1.75;          /* the ink dots at the top and the foot of the y axis */
const THIS = Object.freeze({ step: 2.1, lw: 1.3, gap: 3.2, between: 0.55, head: 3.6 });
const LAST = Object.freeze({ lw: 1.2, head: 2.1 });
const GROW = 0.45, GROW_BOUNCE = 0.1;
const TAU = Math.PI * 2;
const MINUS = '−';

const CSS = '.vk-months .vk-readout.vk-tight{gap:0 10px}';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

/* ---------- numbers and words ---------- */
function numberFormat(min, max) {
  try { return new Intl.NumberFormat('en-US', { minimumFractionDigits: min, maximumFractionDigits: max, useGrouping: true }); } catch (err) { return null; }
}
const NF0 = numberFormat(0, 0), NF1 = numberFormat(0, 1), NF2 = numberFormat(2, 2), NFP = numberFormat(0, 2);
const fmtWith = (nf, n, digits) => (nf ? nf.format(n) : n.toFixed(digits));
const sign = (n) => (n < 0 ? MINUS : '');
/* $2,512 for a total of $100 or more, $45.20 under it, with a minus sign below zero. */
function money(n) {
  const a = Math.abs(n);
  const cents = a < 100 && Math.round(a * 100) % 100 !== 0;
  return `${sign(n)}$${cents ? fmtWith(NF2, a, 2) : fmtWith(NF0, Math.round(a), 0)}`;
}
/* 41,200 for a total of 100 or more, 12.75 under it: always grouped, so a difference of 2,250 reads like the totals. */
function plain(n) {
  const a = Math.abs(n);
  return `${sign(n)}${a >= 100 ? fmtWith(NF0, Math.round(a), 0) : fmtWith(NFP, a, 2)}`;
}
/* The unit: { money } for USD, or { word } for any other word, or neither. */
function unitOf(out) {
  const u = out && typeof out.unit === 'string' ? out.unit.trim().slice(0, UNIT_MAX) : '';
  if (/^usd$/i.test(u)) return { money: true, word: '' };
  return { money: false, word: u };
}
/* A total as the readout and the figure's label say it: $2,629, 41,200 steps, or 41,200. */
const figure = (n, U) => (U.money ? money(n) : U.word ? `${plain(n)} ${U.word}` : plain(n));
/* A difference: $137 or 2,250, without the unit word, as in "2,250 more than last week". */
const amount = (n, U) => (U.money ? money(n) : plain(n));
/* The axis keeps one form: $2,000 or 40,000, then $5k or 500k once the top tick is too long for the gutter, and M
   from a million. */
function axisLabel(n, U, compact) {
  if (!compact) return U.money ? money(n) : plain(n);
  const big = compact === 'M', v = n / (big ? 1e6 : 1e3);
  const s = n === 0 ? '0' : `${sign(n)}${Number.isInteger(v) ? fmtWith(NF0, Math.abs(v), 0) : fmtWith(NF1, Math.abs(v), 1)}${big ? 'M' : 'k'}`;
  return U.money ? (n === 0 ? '$0' : s.replace(/^(−?)/, '$1$')) : s;
}
function compactOf(yMax, U) {
  if (yMax >= 1e6) return 'M';
  return yMax >= (U.money ? 1e4 : 1e5) ? 'k' : '';
}
const ordinal = (d) => {
  const t = d % 100;
  const s = t >= 11 && t <= 13 ? 'th' : ['th', 'st', 'nd', 'rd'][d % 10] || 'th';
  return `${d}${s}`;
};
const hour12 = (h) => `${h % 12 || 12} ${h < 12 ? 'AM' : 'PM'}`;
/* A step in a sentence, after "By": the 17th, Wednesday, 3 PM. */
function stepWords(kind, s) {
  if (kind === 'weekday') return WEEKDAYS[s - 1];
  if (kind === 'hour') return hour12(s);
  return `the ${ordinal(s)}`;
}
/* The x axis's labels: the 1st and every fifth day (every tenth when narrow); Mon to Sun (M to S when narrow); every
   sixth hour from 12 AM (every twelfth when narrow). */
function axisSteps(M, perStep) {
  const list = [];
  if (M.kind === 'weekday') {
    const short = perStep >= 24;
    for (let s = M.first; s <= M.last; s++) list.push([s, short ? WEEKDAYS[s - 1].slice(0, 3) : WEEKDAYS[s - 1][0]]);
  } else if (M.kind === 'hour') {
    const every = perStep * 6 >= 36 ? 6 : 12;
    for (let s = M.first; s <= M.last; s += every) list.push([s, hour12(s)]);
  } else {
    const every = perStep * 5 >= 24 ? 5 : 10;
    list.push([1, '1']);
    for (let s = every; s <= M.last; s += every) list.push([s, String(s)]);
  }
  return list;
}

/* ---------- the model ---------- */
function stepOf(v, kind) {
  const K = STEPS[kind];
  if (isNum(v)) return Number.isInteger(v) && v >= K.lo && v <= K.hi ? v : null;
  if (typeof v === 'string') {
    const s = v.trim();
    if (/^\d{1,2}$/.test(s)) return stepOf(Number(s), kind);
    const t = kit.parseTime(s);
    if (t) return kind === 'hour' ? t.getHours() : kind === 'weekday' ? ((t.getDay() + 6) % 7) + 1 : t.getDate();
  }
  return null;
}

/* A total's name inside a sentence: a column name in snake case is spaced out; an alias keeps its words, with its
   first letter lowered unless it starts a name (October, Monday) or an abbreviation. */
const PROPER = /^(jan|feb|mar|apr|may|jun|jul|aug|sep|oct|nov|dec|mon|tue|wed|thu|fri|sat|sun)[a-z]*\b/i;
function inSentence(key, fallback) {
  if (!key) return fallback;
  if (/^[a-z0-9_]+$/.test(key)) return key.replace(/_+/g, ' ').trim();
  const label = kit.columnLabel(key);
  if (PROPER.test(label) || /^[A-Z][A-Z0-9]/.test(label)) return label;
  return label.charAt(0).toLowerCase() + label.slice(1);
}

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const kind = out && Object.prototype.hasOwnProperty.call(STEPS, out.step_kind) ? out.step_kind : 'day';
  const K = STEPS[kind];
  const stepKey = columns.includes('step') ? 'step' : columns.includes('day') ? 'day' : columns[0];
  const rest = columns.filter((c) => c !== stepKey);
  const aKey = rest.includes('this_month') ? 'this_month' : rest[0];
  const bKey = rest.includes('last_month') ? 'last_month' : rest.find((c) => c !== aKey);
  const byStep = new Map();
  rows.forEach((r, i) => {
    const step = stepOf(r[stepKey], kind);
    if (step === null || byStep.has(step)) return;
    byStep.set(step, { step, a: isNum(r[aKey]) ? r[aKey] : null, b: isNum(r[bKey]) ? r[bKey] : null, i });
  });
  const pts = [...byStep.values()].sort((p, q) => p.step - q.step);
  const first = K.lo;
  const last = Math.max(K.lo + K.span - 1, pts.length ? pts[pts.length - 1].step : K.hi);
  let hi = 0;
  for (const p of pts) for (const v of [p.a, p.b]) if (v !== null) hi = Math.max(hi, v);
  let ticks = kit.niceTicks(0, hi);
  if (ticks.length < 2) ticks = [0, hi || 1];
  let head = -1;
  pts.forEach((p, k) => { if (p.a !== null) head = k; });
  /* how each total is named: the column's label on screen, and in a sentence */
  const names = {
    a: aKey ? kit.columnLabel(aKey) : 'This period', b: bKey ? kit.columnLabel(bKey) : 'Last period',
    aText: inSentence(aKey, 'this period'), bText: inSentence(bKey, 'last period'),
  };
  const unit = unitOf(out);
  const yMax = ticks[ticks.length - 1];
  return { columns, kind, K, pts, first, last, ticks, yMax, head, names, unit, compact: compactOf(yMax, unit) };
}

/* "$142 more than last month" or "2,250 more than last week", in plain words and never a judgment. */
function difference(M, d) {
  const same = M.unit.money || Math.abs(M.yMax) >= 100 ? Math.abs(d) < 0.5 : Math.abs(d) < 0.005;
  if (same) return { long: `The same as ${M.names.bText}`, short: 'The same' };
  const word = d > 0 ? 'more' : 'less', n = amount(Math.abs(d), M.unit);
  return { long: `${n} ${word} than ${M.names.bText}`, short: `${n} ${word}` };
}

function ariaLabel(M) {
  const { aText, bText } = M.names, U = M.unit;
  const what = U.money ? 'Running totals in dollars' : U.word ? `Running totals in ${U.word}` : 'Running totals';
  let label = `${what} by ${M.K.of}: ${aText} as a line of dots and ${bText} as a thin line`;
  const p = M.head >= 0 ? M.pts[M.head] : null;
  if (p) {
    label += `. By ${stepWords(M.kind, p.step)}, ${aText} comes to ${figure(p.a, U)}`;
    if (p.b !== null) label += ` and ${bText} came to ${figure(p.b, U)} by the same ${M.K.noun}`;
  }
  let lastB = null;
  for (const q of M.pts) if (q.b !== null) lastB = q;
  if (lastB && (!p || lastB.step !== p.step)) label += `. ${M.names.b} came to ${figure(lastB.b, U)} by ${stepWords(M.kind, lastB.step)}`;
  return `${label}.`;
}

/* The readout's marks: this period's dots and the previous period's pencil line, each with its name for screen
   readers when the line has no room to show it. */
function thisSwatch(sr) {
  const dots = [2, 8, 14].map((x) => `<circle cx="${x}" cy="4" r="1.9"/>`).join('');
  const fine = [5, 11].map((x) => `<circle cx="${x}" cy="4" r=".65"/>`).join('');
  return `<svg class="vk-sw" width="16" height="8" viewBox="0 0 16 8" aria-hidden="true"><g style="fill:var(--vk-s0)">${dots}${fine}</g></svg>${sr ? `<span class="vk-sr">${kit.esc(sr)}</span>` : ''}`;
}
function lastSwatch(sr) {
  return `<svg class="vk-sw" width="16" height="8" viewBox="0 0 16 8" aria-hidden="true"><line x1="1" y1="4" x2="15" y2="4" style="stroke:var(--vk-s1);stroke-width:${LAST.lw};stroke-linecap:round"/></svg>${sr ? `<span class="vk-sr">${kit.esc(sr)}</span>` : ''}`;
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
  kit.ensureCss('months', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-months');
  el.appendChild(root);
  const blk = kit.block(root, {
    plotHeight: HEIGHT, readout: true, foot: opts.foot !== false,
    hint: 'Use the left and right arrow keys to read each day.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  blk.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 };
  /* The reveal: 'none' draws everything at once; 'live' raises each step as the frame's sweep passes it. */
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const B = () => box(width);
  const Y = (v) => {
    const b = B(), plotH = b.bottom - b.top;
    return b.top + plotH - (v / (M.yMax || 1)) * plotH;
  };
  const X = (step) => {
    const b = B(), x0 = b.left + INSET;
    return x0 + ((step - M.first) / Math.max(1, M.last - M.first)) * (b.right - x0);
  };
  const xAt = (k) => X(M.pts[k].step);

  function growth(k) {
    if (mode === 'none') return 1;
    if (born[k] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[k], GROW, GROW_BOUNCE));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (ctx, x, y, r) => { ctx.moveTo(x + r, y); ctx.arc(x, y, r, 0, TAU); };

  /* The runs of steps where `key` has a total, each point [k, x, y]. */
  function runs(key) {
    const floor = B().bottom;
    const list = [];
    let run = null;
    M.pts.forEach((p, k) => {
      const v = p[key], g = growth(k);
      if (v === null || g <= 0) { run = null; return; }
      if (!run) list.push(run = []);
      run.push([k, X(p.step), floor + (Y(v) - floor) * g]);
    });
    return list;
  }

  function draw() {
    if (!width || !M.pts.length) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const T = tokens, b = B();
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;

    /* the grid and the floor arrive behind the sweep */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    for (const t of M.ticks) {
      const y = Math.round(Y(t));
      if (Math.abs(y - floor) < 1) continue;
      dotted(ctx, L, y + 0.5, R, y + 0.5, 5, 1.1, T.borderStrong);
    }
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(L, floor, R - L, 1);
    ctx.restore();

    /* the steps: days of the month, days of the week or hours, as axisSteps chooses for the width */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'alphabetic';
    const perStep = (R - L - INSET) / Math.max(1, M.last - M.first);
    for (const [s, text] of axisSteps(M, perStep)) {
      const x = X(s), w = ctx.measureText(text).width;
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - x, 0, 24) : 1);
      if (a <= 0) continue;
      ctx.globalAlpha = a;
      ctx.fillText(text, kit.clamp(x - w / 2, 1, width - 1 - w), HEIGHT - 6);
    }
    ctx.restore();

    /* the y axis: fine dots from the foot to the top with an ink dot at each end */
    dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); disc(ctx, L, top, END_R); disc(ctx, L, floor, END_R); ctx.fill();
    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    for (const t of M.ticks) ctx.fillText(axisLabel(t, M.unit, M.compact), L - 6, Y(t));
    /* a unit word is written once, right of the axis on the top tick's line, where a running total has not reached */
    if (M.unit.word) {
      const y = Y(M.yMax), w = ctx.measureText(M.unit.word).width;
      ctx.fillStyle = T.page;
      ctx.fillRect(L + 3, y - 6, w + 7, 12);
      ctx.fillStyle = T.muted;
      ctx.textAlign = 'left';
      ctx.fillText(M.unit.word, L + 6, y);
    }
    ctx.restore();

    const hv = hover;
    if (hv !== null) dotted(ctx, xAt(hv), top, xAt(hv), floor, 4, 1, T.borderStrong);

    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }

    /* the previous period: the pencil line */
    const bRuns = runs('b');
    ctx.strokeStyle = T.s1; ctx.lineWidth = LAST.lw; ctx.lineCap = 'round'; ctx.lineJoin = 'round';
    ctx.beginPath();
    for (const run of bRuns) run.forEach(([, x, y], j) => (j ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
    ctx.stroke();

    /* this period: fine dots along the line, a dot on each step, and a larger dot on the latest step */
    const aRuns = runs('a');
    const s0 = T.rgb.s0;
    ctx.strokeStyle = kit.rgba(s0, s0[3] * THIS.between);
    ctx.lineWidth = THIS.lw; ctx.setLineDash([0, THIS.gap]);
    ctx.beginPath();
    for (const run of aRuns) run.forEach(([, x, y], j) => (j ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
    ctx.stroke();
    ctx.setLineDash([]);
    const steps = new Path2D();
    let headAt = null;
    for (const run of aRuns) {
      for (const [k, x, y] of run) {
        if (k === M.head) { headAt = [x, y, growth(k)]; continue; }
        const r = THIS.step * kit.clamp(growth(k) * 3, 0, 1);
        if (r > 0) { steps.moveTo(x + r, y); steps.arc(x, y, r, 0, TAU); }
      }
    }
    ctx.fillStyle = T.s0;
    ctx.fill(steps);
    if (headAt) {
      const r = THIS.head * kit.clamp(headAt[2] * 3, 0, 1);
      ctx.fillStyle = T.page;
      ctx.beginPath(); disc(ctx, headAt[0], headAt[1], r + 1.2); ctx.fill();
      ctx.fillStyle = T.s0;
      ctx.beginPath(); disc(ctx, headAt[0], headAt[1], r); ctx.fill();
    }
    ctx.restore();

    /* the step you rest on: a hairline between the two totals, and a ring on each */
    if (hv !== null) {
      const p = M.pts[hv], x = xAt(hv), g = growth(hv);
      const ya = p.a !== null ? floor + (Y(p.a) - floor) * g : null;
      const yb = p.b !== null ? floor + (Y(p.b) - floor) * g : null;
      if (ya !== null && yb !== null && Math.abs(ya - yb) > 10) {
        ctx.fillStyle = T.muted;
        ctx.fillRect(Math.round(x * 2) / 2 - 0.5, Math.min(ya, yb) + 5, 1, Math.abs(ya - yb) - 10);
      }
      ctx.save();
      ctx.lineWidth = 1;
      if (yb !== null) {
        ctx.strokeStyle = T.s1;
        ctx.fillStyle = T.s1;
        ctx.beginPath(); disc(ctx, x, yb, LAST.head); ctx.fill();
        ctx.beginPath(); ctx.arc(x, yb, LAST.head + 1.5 + 2.5 * ring.x, 0, TAU); ctx.stroke();
      }
      if (ya !== null) {
        const s = T.rgb.s0;
        ctx.strokeStyle = kit.rgba(s, Math.min(1, s[3] * 1.1) * 0.9);
        const r0 = hv === M.head ? THIS.head : THIS.step;
        ctx.fillStyle = T.s0;
        ctx.beginPath(); disc(ctx, x, ya, r0 + 0.5); ctx.fill();
        ctx.beginPath(); ctx.arc(x, ya, r0 + 1.5 + 3.5 * ring.x, 0, TAU); ctx.stroke();
      }
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    const node = blk.readout;
    if (!node) return;
    const { a, b } = M.names;
    if (hover === null || !M.pts[hover]) {
      fitReadout(node, [[{ swatch: thisSwatch(), label: a }, { swatch: lastSwatch(), label: b }]]);
      return;
    }
    const p = M.pts[hover], U = M.unit;
    const when = { when: `By ${stepWords(M.kind, p.step)}` };
    const diff = p.a !== null && p.b !== null ? difference(M, p.a - p.b) : null;
    const full = [when], mid = [when], tight = [when];
    if (p.a !== null) {
      full.push({ swatch: thisSwatch(), label: a, value: figure(p.a, U) });
      mid.push({ swatch: thisSwatch(a), value: figure(p.a, U) });
      tight.push({ swatch: thisSwatch(a), value: figure(p.a, U) });
    }
    if (p.b !== null) {
      full.push({ swatch: lastSwatch(), label: b, value: figure(p.b, U) });
      mid.push({ swatch: lastSwatch(b), value: figure(p.b, U) });
      tight.push({ swatch: lastSwatch(b), value: figure(p.b, U) });
    }
    if (p.a === null && p.b === null) { full.push({ value: '-' }); mid.push({ value: '-' }); tight.push({ value: '-' }); }
    if (diff) {
      full.push({ label: diff.long });
      mid.push({ label: diff.short });
      tight.push({ swatch: `<span class="vk-sr">${kit.esc(diff.long)}</span>` });
    }
    fitReadout(node, [full, mid, tight]);
  }

  function setHover(k, from) {
    if (k === hover) { hoverFrom = from; return; }
    hover = k; hoverFrom = from;
    if (k !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows or steps rise ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    if (reduced()) { ring.x = 1; ring.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      moving = ring.x !== 1;
    }
    if (mode === 'live' && !reduced()) {
      if (clock - revealAt < 0.35) moving = true;
      for (let k = 0; k < born.length; k++) if (born[k] !== undefined && clock - born[k] < 1.2) { moving = true; break; }
      if (reveal < Infinity) moving = true;
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    for (let k = 0; k < M.pts.length; k++) {
      if (born[k] === undefined && (reveal === Infinity || xAt(k) <= reveal)) born[k] = clock;
    }
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
  function nearest(px) {
    let best = 0, dist = Infinity;
    for (let k = 0; k < M.pts.length; k++) { const d = Math.abs(xAt(k) - px); if (d < dist) { best = k; dist = d; } }
    return best;
  }
  /* A mouse reads the step under it and lets go when it leaves. A finger lifts as soon as it taps, so a tap keeps its
     step until the same step is tapped again or the chart loses focus. A drag across the chart scrubs. */
  const pick = (e) => nearest(e.clientX - canvas.getBoundingClientRect().left);
  const onMove = (e) => {
    if (!width || !M.pts.length) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.pts.length) return;
    const k = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === k) { setHover(null, null); return; }
    setHover(k, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  blk.plot.addEventListener('pointermove', onMove);
  blk.plot.addEventListener('pointerdown', onDown);
  blk.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(blk.plot, {
    count: () => M.pts.length,
    get: () => hover,
    page: 7,
    set: (k, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(k, 'key');
    },
  });

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    born = [];
    hover = null; hoverFrom = null;
    kit.setLabel(blk.plot, ariaLabel(M));
    blk.hint.textContent = `Use the left and right arrow keys to read each ${M.K.noun}.`;
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
