// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/cloud.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* cloud.js: a scatter plot, as a cloud of records that lines up when you turn it. It answers whether two measures
   move together, or when in the week something tends to happen. A new chart mark: kind "chart", mark "scatter".

   Each dot is one row of the query, set in a box by three measures. From most angles the box looks like weather.
   Turn it to look straight down its depth and the dots fall into an exact two-dimensional chart, which gives show
   its first scatter. There are two forms, and the columns choose between them:
   - relation (two measures): a first column naming each record (a date, a time or a label), then two numeric columns; two numeric
     columns alone also work. The first measure is across, the second is up, and the depth is the day's date, or
     the row order when the first column is not a date. Lined up it is a scatter chart: the least-squares line
     draws through the days, with dotted guides one residual spread above and below it.
   - rhythm (a weekly rhythm): one column of times, each row a moment, such as an hour you were out walking. Across is the weekday, up is
     the hour of the day with the morning at the top, and the depth is the week. Lined up, the weeks fall onto each
     other and the moments fall into columns, one per weekday. Where moments overlap the dots darken.
   When the cloud lines up, the result's `finding` (one sentence the model wrote) writes itself word by word in the
   row under the plot. Before that the row asks you to turn the cloud.

   The turning is lab 7's (lab-7/t05-align.js) and lab 12's (lab-12/t-12-h-weeks.js): the orientation is a
   quaternion kept as its log, a drag composes turns about the screen's axes at 0.008 rad per px, and a detent
   catches the view within 8 degrees of looking down the depth and pulls it home on a soft spring (0.35 s, bounce
   0.12). It lets go past 13 degrees. A flick coasts and can coast into the detent. Turn to see turns it for you in
   1.1 s, and Turn it back returns it. A finger on a phone turns the cloud along the path between its first view and
   the found one, so one sideways drag always reaches it and an up or down drag still scrolls the reply. The view is
   orthographic, with depth shown by dot size and ink, so the found view is exact. Turned, the box keeps its own
   proportions, a little wider than tall and as deep as it is wide, so the days spread in depth as much as across.
   As it lines up it grows and stretches until its face is exactly the plot rect. On the lock four brackets close on
   the face and let go.

   Under reduced motion, and when you reach the figure with the keyboard, it is already lined up. The arrow keys
   turn it. Once it lines up, left and right read each dot in order across, and up and down turn it away.

   Contract: see the top of kit.js. Block: readout 26 + plot 240 + finding 56 + foot 26 = 348px. The plot is taller
   than a chart's 200px because the turned box needs depth, and the aligned scatter reads better with the height. The
   finding row is always held, so nothing moves when the words arrive. Turn to see sits at the right end of the foot,
   beside See the rows, as the ridges' sort does. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'cloud', kinds: ['chart'], marks: ['scatter'] });

const PLOT_H = 240;
const PAD = Object.freeze({ top: 12, right: 14, bottom: 24, left: 44 });
const SAY_H = 56;            /* an 8px gap and two 24px lines of the 18px serif */
const MAX_POINTS = 400;      /* the row cap proposed for show.rs */
const TAU = Math.PI * 2, D2R = Math.PI / 180;
const ENGAGE = 8, LET_GO = 13;     /* degrees from the found view: lab 7 caught at 9, lab 12 at 6 */
const TURN_S = 1.1;
const DRAG = 0.008;
const KEY_STEP = 15 * D2R;       /* one arrow press: past the detent's 13 degrees, so one press turns it away */
const END_R = 1.75;
const PROMPT = 'Turn the cloud until it lines up.';
const TURN_TEXT = 'Turn to see', BACK_TEXT = 'Turn it back';
const HINT = 'Drag to turn the cloud, or use the arrow keys. Once it lines up, the left and right arrow keys read each dot, and the up and down arrow keys turn it away.';

const CSS = `
.vk-cloud .vk-plot{cursor:grab;user-select:none;-webkit-user-select:none}
.vk-cloud .vk-plot.vk-cloud-drag{cursor:grabbing}
.vk-cloud .vk-cloud-say{position:relative;height:48px;margin:8px 0 0;font-family:var(--vk-serif);font-style:normal;font-weight:400;font-size:18px;line-height:24px;overflow:hidden}
.vk-cloud .vk-cloud-pre,.vk-cloud .vk-cloud-find{position:absolute;left:0;top:0;right:0;margin:0;font:inherit;font-style:normal}
.vk-cloud .vk-cloud-pre{color:var(--vk-muted);transition:opacity .22s cubic-bezier(.2,0,0,1) .14s}
.vk-cloud .vk-cloud-pre.vk-off{opacity:0;transition:opacity .1s ease 0s}
.vk-cloud .vk-cloud-find{color:var(--vk-ink)}
.vk-cloud .vk-cloud-find span{opacity:0;transition:opacity .1s ease 0s}
.vk-cloud .vk-cloud-find.vk-on span{opacity:1;transition:opacity .3s cubic-bezier(.2,0,0,1);transition-delay:var(--d,0s)}
.vk-cloud.vk-still .vk-cloud-pre,.vk-cloud.vk-still .vk-cloud-find span{transition:none}
.vk-cloud .vk-cloud-turn{all:unset;margin-left:auto;flex:none;cursor:pointer;font:inherit;line-height:18px;padding:0 2px;border-radius:2px;color:var(--vk-primary);white-space:nowrap}
.vk-cloud .vk-cloud-turn:hover{text-decoration:underline;text-underline-offset:3px;text-decoration-thickness:1px}
.vk-cloud .vk-cloud-turn:focus-visible{outline:2px solid var(--vk-focus);outline-offset:1px}
.vk-cloud .vk-cloud-turn.vk-cloud-top{position:absolute;top:0;right:0;font-size:13px;line-height:20px}
.vk-cloud .vk-cloud-turn[hidden]{display:none}
@media (pointer:coarse){.vk-cloud .vk-cloud-turn{position:relative}.vk-cloud .vk-cloud-turn::after{content:'';position:absolute;left:-10px;right:-10px;top:-12px;bottom:-12px}}
@media (prefers-reduced-motion:reduce){.vk-cloud .vk-cloud-pre,.vk-cloud .vk-cloud-find span{transition:none}}
`;

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + PLOT_H + SAY_H + (options.foot === false ? 0 : kit.ROWS.foot);
}

/* The lined-up chart's plot rect, where the frame runs its dot while the query runs. */
export function box(width) {
  return kit.plotBox(width, PLOT_H, PAD);
}

/* ---------- quaternions [w, x, y, z], as the parents keep them ---------- */
const qmul = (a, b) => [
  a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
  a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
  a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
  a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
];
const qaxis = (ax, an) => { const s = Math.sin(an / 2); return [Math.cos(an / 2), ax[0] * s, ax[1] * s, ax[2] * s]; };
const qexp = (e) => {
  const th = Math.hypot(e[0], e[1], e[2]);
  if (th < 1e-9) return [1, 0, 0, 0];
  return qaxis([e[0] / th, e[1] / th, e[2] / th], th);
};
const qlog = (q) => {
  let [w, x, y, z] = q;
  if (w < 0) { w = -w; x = -x; y = -y; z = -z; }
  const s = Math.hypot(x, y, z);
  if (s < 1e-9) return [0, 0, 0];
  const th = 2 * Math.atan2(s, w);
  return [x / s * th, y / s * th, z / s * th];
};
const qrot = (q, v) => {
  const [w, x, y, z] = q;
  const ix = w * v[0] + y * v[2] - z * v[1], iy = w * v[1] + z * v[0] - x * v[2], iz = w * v[2] + x * v[1] - y * v[0], iw = -x * v[0] - y * v[1] - z * v[2];
  return [ix * w + iw * -x + iy * -z - iz * -y, iy * w + iw * -y + iz * -x - ix * -z, iz * w + iw * -z + ix * -y - iy * -x];
};
const angOf = (e) => Math.hypot(e[0], e[1], e[2]) / D2R;
/* How far the view is from looking straight down the depth. A drag cannot undo a roll about the view axis, and a
   roll does not hide the alignment, so the detent measures direction only; its spring then removes the roll. */
const offOf = (e) => { const c = qrot(qexp(e), [0, 0, 1]); return Math.acos(kit.clamp(c[2], -1, 1)) / D2R; };
/* The first view: turned 34 degrees about the vertical, then tipped 22 degrees toward you. */
const START = qlog(qmul(qaxis([1, 0, 0], 22 * D2R), qaxis([0, 1, 0], -34 * D2R)));

/* ---------- words ---------- */
function formatter(opts) {
  try { return new Intl.DateTimeFormat(undefined, opts); } catch (err) { return null; }
}
const DAY_FMT = formatter({ weekday: 'short', month: 'short', day: 'numeric' });
const WEEKDAY_FMT = formatter({ weekday: 'short' });
const TIME_FMT = formatter({ hour: 'numeric', minute: '2-digit' });
const HOUR_FMT = formatter({ hour: 'numeric' });
const dayText = (t) => (DAY_FMT ? DAY_FMT.format(t) : kit.formatTime(t, { withDate: true, withTime: false }));
const dateText = (t) => kit.formatTime(t, { withDate: true, withTime: false });
const timeText = (t) => (TIME_FMT ? TIME_FMT.format(t) : kit.formatTime(t));
/* Monday is January 3, 2000, so weekday d (0 Monday) names itself in the reader's language. */
const weekdayName = (d) => (WEEKDAY_FMT ? WEEKDAY_FMT.format(new Date(2000, 0, 3 + d)) : ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'][d]);
function hourName(h) {
  const t = new Date(2000, 0, 3, Math.floor(h), Math.round((h % 1) * 60));
  const f = h % 1 ? TIME_FMT : HOUR_FMT;
  return f ? f.format(t) : kit.formatTime(t);
}
const range = (a, b) => (a === b ? a : `${a} to ${b}`);

/* ---------- the model: which form, and every dot's place in the unit box ---------- */
const DAY_MS = 864e5;
const EPOCH = new Date(2000, 0, 3).getTime();   /* a Monday */
const dayIndex = (t) => Math.round((new Date(t.getFullYear(), t.getMonth(), t.getDate()).getTime() - EPOCH) / DAY_MS);
/* A stable jitter per row, so the turned view never reshuffles. */
const hash01 = (i) => { let x = (i + 1) * 2654435761 % 4294967296; x ^= x >>> 15; x = Math.imul(x, 2246822519) >>> 0; x ^= x >>> 13; return (x >>> 0) / 4294967296; };

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = (Array.isArray(out && out.rows) ? out.rows : []).slice(0, MAX_POINTS);
  const finding = out && typeof out.finding === 'string' ? out.finding.trim() : '';
  const M = { columns, rows, finding, form: null, pts: [], order: [] };
  const numeric = (c) => kit.isNumericColumn(rows, c);
  if (columns.length === 1) {
    const vals = rows.map((r) => r[columns[0]]);
    if (kit.isTimeColumn(vals) && !kit.isDateColumn(vals)) rhythm(M, columns[0]);
  } else if (columns.length === 2 && numeric(columns[0]) && numeric(columns[1])) relation(M, null, columns[0], columns[1]);
  else if (columns.length === 3 && numeric(columns[1]) && numeric(columns[2])) relation(M, columns[0], columns[1], columns[2]);
  return M;
}

function ticksFor(lo, hi) {
  let t = kit.niceTicks(lo, hi);
  if (t.length < 2) t = [lo, hi === lo ? lo + 1 : hi];
  return t;
}

function relation(M, key, A, B) {
  const keys = key ? M.rows.map((r) => r[key]) : [];
  const dates = !!key && (kit.isDateColumn(keys) || kit.isTimeColumn(keys));
  const pts = [];
  M.rows.forEach((row, i) => {
    const a = row[A], b = row[B];
    if (typeof a !== 'number' || !Number.isFinite(a) || typeof b !== 'number' || !Number.isFinite(b)) return;
    const t = dates ? kit.parseTime(row[key]) : null;
    if (dates && !t) return;
    pts.push({ i, row, a, b, t, k: key ? row[key] : null });
  });
  if (pts.length < 2) return;
  const as = pts.map((p) => p.a), bs = pts.map((p) => p.b);
  const aLo = Math.min(...as), aHi = Math.max(...as), bLo = Math.min(...bs), bHi = Math.max(...bs);
  const ticksA = ticksFor(aLo, aHi), ticksB = ticksFor(bLo, bHi);
  const a0 = ticksA[0], a1 = ticksA[ticksA.length - 1], b0 = ticksB[0], b1 = ticksB[ticksB.length - 1];
  const depth = pts.map((p, j) => (p.t ? p.t.getTime() : j));
  const z0 = Math.min(...depth), z1 = Math.max(...depth);
  pts.forEach((p, j) => {
    p.ux = 2 * (p.a - a0) / (a1 - a0 || 1) - 1;
    p.uy = 2 * (p.b - b0) / (b1 - b0 || 1) - 1;
    p.uz = z1 === z0 ? 0 : 2 * (depth[j] - z0) / (z1 - z0) - 1;   /* the latest day nearest you */
    p.jx = 0;
  });
  /* the least-squares line of the second measure on the first, and the spread of the days about it */
  const n = pts.length, mx = as.reduce((s, v) => s + v, 0) / n, my = bs.reduce((s, v) => s + v, 0) / n;
  let sxx = 0, sxy = 0;
  for (const p of pts) { sxx += (p.a - mx) ** 2; sxy += (p.a - mx) * (p.b - my); }
  let fit = null;
  if (n >= 3 && sxx > 0) {
    const slope = sxy / sxx, icept = my - slope * mx;
    let sse = 0;
    for (const p of pts) sse += (p.b - (icept + slope * p.a)) ** 2;
    fit = { slope, icept, spread: Math.sqrt(sse / (n - 2)), lo: aLo, hi: aHi };
  }
  const ux = (a) => 2 * (a - a0) / (a1 - a0 || 1) - 1, uy = (b) => 2 * (b - b0) / (b1 - b0 || 1) - 1;
  const order = pts.map((p, j) => j).sort((x, y) => pts[x].a - pts[y].a || pts[x].b - pts[y].b);
  let depthText = '';
  if (dates) {
    const ts = pts.map((p) => p.t.getTime());
    depthText = range(dateText(new Date(Math.min(...ts))), dateText(new Date(Math.max(...ts))));
  }
  Object.assign(M, {
    form: 'relation', pts, order, key, A, B, dates, fit, ticksA, ticksB, ux, uy, depthText,
    xLabels: ticksA.map((t) => ({ u: ux(t), text: kit.formatNumber(t) })),
    yLabels: ticksB.map((t) => ({ u: uy(t), text: kit.formatNumber(t) })),
  });
}

function rhythm(M, col) {
  const pts = [];
  M.rows.forEach((row, i) => {
    const t = kit.parseTime(row[col]);
    if (!t) return;
    const wd = (t.getDay() + 6) % 7;
    pts.push({ i, row, t, wd, hour: t.getHours() + t.getMinutes() / 60 + t.getSeconds() / 3600, di: dayIndex(t) });
  });
  if (pts.length < 2) return;
  const hours = pts.map((p) => p.hour);
  /* the hours on whole three-hour ticks, at least six hours tall */
  let lo = Math.floor(Math.min(...hours) / 3) * 3, hi = Math.ceil(Math.max(...hours) / 3) * 3;
  if (hi - lo < 6) { const mid = (lo + hi) / 2; lo = Math.max(0, Math.floor((mid - 3) / 3) * 3); hi = Math.min(24, lo + 6); lo = hi - 6; }
  const monday = Math.min(...pts.map((p) => p.di - p.wd));
  pts.forEach((p) => { p.week = Math.floor((p.di - monday) / 7); });
  const weeks = Math.max(...pts.map((p) => p.week)) + 1;
  pts.forEach((p) => {
    p.ux = -1 + (2 / 7) * (p.wd + 0.5);
    p.uy = 1 - 2 * (p.hour - lo) / (hi - lo);
    p.uz = weeks > 1 ? -1 + 2 * p.week / (weeks - 1) : 0;   /* the latest week nearest you */
    p.jx = (hash01(p.i) - 0.5) * (2 / 7) * 0.4;
  });
  const order = pts.map((p, j) => j).sort((x, y) => pts[x].wd - pts[y].wd || pts[x].hour - pts[y].hour || pts[x].t - pts[y].t);
  const ts = pts.map((p) => p.t.getTime());
  const first = new Date(Math.min(...ts)), last = new Date(Math.max(...ts));
  const yLabels = [];
  for (let h = Math.ceil(lo / 3) * 3; h <= hi + 1e-9; h += 3) yLabels.push({ u: 1 - 2 * (h - lo) / (hi - lo), text: hourName(h % 24 === 0 && h > 0 ? 0 : h) });
  Object.assign(M, {
    form: 'rhythm', pts, order, col, lo, hi, weeks, first, last,
    depthText: range(dateText(first), dateText(last)),
    xLabels: Array.from({ length: 7 }, (_, d) => ({ u: -1 + (2 / 7) * (d + 0.5), text: weekdayName(d) })),
    yLabels,
  });
}

function ariaText(M, aligned) {
  if (!M.form) return 'Scatter plot with no rows it can place';
  const state = aligned ? 'Lined up, it reads as a flat chart.' : 'Turned, the dots are a cloud; turn it to line them up.';
  if (M.form === 'relation') {
    const per = M.key ? `, one dot per ${kit.columnLabel(M.key)}` : '';
    const when = M.depthText ? `, ${M.depthText}` : '';
    return `Scatter plot of ${kit.columnLabel(M.B)} against ${kit.columnLabel(M.A)}${per}${when}. ${state}`;
  }
  return `Scatter plot of ${kit.columnLabel(M.col)} by weekday and hour, ${M.depthText}. ${state}`;
}

/* ---------- the component ---------- */
export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('cloud', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-cloud');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: PLOT_H, readout: true, foot: opts.foot !== false, hint: HINT });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);
  const say = kit.h('div', 'vk-cloud-say');
  const pre = kit.h('p', 'vk-cloud-pre', { 'aria-hidden': 'true' });
  pre.textContent = PROMPT;
  const find = kit.h('p', 'vk-cloud-find');
  say.append(pre, find);
  if (parts.foot) parts.root.insertBefore(say, parts.foot); else parts.root.appendChild(say);
  const turnBtn = kit.h('button', 'vk-cloud-turn', { type: 'button' });
  turnBtn.textContent = TURN_TEXT;

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  const st = { raw: START.slice(), e: START.map((x) => ({ x, v: 0 })), engaged: false, engT: -9, drag: null, tween: null, coast: null, lineU: 0, jit: 1, g: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  let proj = [];
  let wordsQuiet = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);
  const eD = () => st.e.map((s) => s.x);

  /* ---------- the words under the plot ---------- */
  function setWords(on, quiet) {
    const still = quiet || reduced();
    root.classList.toggle('vk-still', still);
    pre.classList.toggle('vk-off', on);
    find.classList.toggle('vk-on', on);
    if (still && !reduced()) {
      cancelAnimationFrame(wordsQuiet);
      wordsQuiet = requestAnimationFrame(() => { wordsQuiet = requestAnimationFrame(() => { wordsQuiet = 0; root.classList.remove('vk-still'); }); });
    }
  }
  function writeFinding() {
    find.textContent = '';
    if (!M.finding) return;
    M.finding.split(/\s+/).forEach((w, i) => {
      if (i) find.appendChild(document.createTextNode(' '));
      const s = document.createElement('span');
      s.style.setProperty('--d', `${120 + i * 40}ms`);
      s.textContent = w;
      find.appendChild(s);
    });
  }
  /* Two lines of the 18px serif; a longer sentence steps down the scale to 17 and 16 before it would clip. */
  function fitSay() {
    for (const el of [find, pre]) {
      el.style.fontSize = '';
      for (const size of [17, 16]) {
        if (el.scrollHeight <= 49) break;
        el.style.fontSize = `${size}px`;
      }
    }
  }

  /* ---------- lining up ---------- */
  function engage(on, quiet) {
    if (on === st.engaged) return;
    st.engaged = on;
    st.engT = clock;
    if (on) { st.raw = [0, 0, 0]; st.coast = null; }
    setWords(on, quiet);
    turnBtn.textContent = on ? BACK_TEXT : TURN_TEXT;
    kit.setLabel(parts.plot, ariaText(M, on));
    if (!on && hoverFrom === 'key') setHover(null, null);
  }
  function checkDetent() {
    const a = offOf(st.raw);
    if (!st.engaged && a < ENGAGE) engage(true);
    else if (st.engaged && a > LET_GO) engage(false);
  }
  function turnBy(ay, ax) {
    st.raw = qlog(qmul(qmul(qaxis([0, 1, 0], ay), qaxis([1, 0, 0], ax)), qexp(st.raw)));
    checkDetent();
  }
  function alignNow() {
    st.tween = null; st.coast = null;
    st.e.forEach((s) => { s.x = 0; s.v = 0; });
    st.raw = [0, 0, 0];
    engage(true, true);
    st.lineU = 1; st.jit = 0; st.engT = -9;
    loop.wake();
  }
  function turnAwayNow() {
    st.tween = null; st.coast = null;
    engage(false, true);
    st.raw = START.slice();
    st.e.forEach((s, i) => { s.x = START[i]; s.v = 0; });
    st.lineU = 0; st.jit = 1;
    loop.wake();
  }
  const onTurn = () => {
    st.coast = null;
    if (!st.engaged) {
      if (reduced()) { alignNow(); return; }
      st.tween = { t: 0, from: eD(), to: [0, 0, 0], dur: TURN_S };
    } else {
      if (reduced()) { turnAwayNow(); return; }
      engage(false);
      st.raw = START.slice();
      st.tween = { t: 0, from: eD(), to: START.slice(), dur: TURN_S };
    }
    loop.wake();
  };
  turnBtn.addEventListener('click', onTurn);

  /* ---------- geometry ---------- */
  function geom() {
    const b = box(width);
    const hw = (b.right - b.left) / 2, hh = (b.bottom - b.top) / 2;
    /* the turned box's proportions: a little wider than tall and as deep as it is wide, so the days spread in
       depth as much as across, whatever the column's width */
    const tx = kit.clamp(hw / hh, 1, 1.45);
    return { b, hw, hh, cx: b.left + hw, cy: b.top + hh, tx, ty: 1, tz: tx };
  }
  const CORNERS = [[-1, -1, -1], [1, -1, -1], [-1, 1, -1], [1, 1, -1], [-1, -1, 1], [1, -1, 1], [-1, 1, 1], [1, 1, 1]];
  /* The camera for the orientation now: how close it is to lined up (tt), and the projection. Turned, the box keeps
     its own proportions, scaled to fit the plot with room for its labels; as it lines up it stretches until its
     face is exactly the plot rect, so the found view is the chart's own geometry. */
  function camera(G) {
    const e = eD(), q = qexp(e);
    const tt = kit.ease.inOut(kit.clamp(1 - angOf(e) / 20, 0, 1));
    let ex = 1e-6, ey = 1e-6, ez = 1e-6;
    for (const c of CORNERS) {
      const p = qrot(q, [c[0] * G.tx, c[1] * G.ty, c[2] * G.tz]);
      ex = Math.max(ex, Math.abs(p[0])); ey = Math.max(ey, Math.abs(p[1])); ez = Math.max(ez, Math.abs(p[2]));
    }
    const fit = Math.min((width / 2 - 24) / ex, (PLOT_H / 2 - 20) / ey);
    const kx = kit.lerp(fit, G.hw / G.tx, tt), ky = kit.lerp(fit, G.hh / G.ty, tt);
    const ox = kit.lerp(width / 2, G.cx, tt), oy = kit.lerp(PLOT_H / 2, G.cy, tt);
    const P = (ux, uy, uz) => {
      const c = qrot(q, [ux * G.tx, uy * G.ty, uz * G.tz]);
      return [ox + c[0] * kx, oy - c[1] * ky, c[2] / ez];
    };
    return { q, tt, P, ez, kx, ky };
  }

  function growth(i) {
    if (mode === 'none') return 1;
    if (born[i] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[i], 0.45, 0.1));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, a, b, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.stroke();
    ctx.restore();
  }
  function line(ctx, a, b, lw, color) {
    ctx.strokeStyle = color; ctx.lineWidth = lw;
    ctx.beginPath(); ctx.moveTo(a[0], a[1]); ctx.lineTo(b[0], b[1]); ctx.stroke();
  }
  /* A label on a plate of the page's own paper, so no dot or edge in a word gap reads as a strike (lab 7). */
  function plate(ctx, text, x, y, align, alpha, T, color) {
    if (alpha <= 0.004) return;
    ctx.font = `9.5px ${T.mono}`;
    const w = ctx.measureText(text).width;
    let x0 = align === 'right' ? x - w : align === 'center' ? x - w / 2 : x;
    x0 = kit.clamp(x0, 3, Math.max(3, width - 3 - w));
    ctx.globalAlpha = Math.min(1, alpha * 1.4);
    ctx.fillStyle = T.page;
    ctx.beginPath(); ctx.roundRect(x0 - 3, y - 8.5, w + 6, 12, 3); ctx.fill();
    ctx.globalAlpha = alpha;
    ctx.fillStyle = color || T.muted;
    ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic';
    ctx.fillText(text, x0, y);
    ctx.globalAlpha = 1;
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, PLOT_H);
    proj = [];
    if (!M.form) return;
    const T = tokens, G = geom(), C = camera(G), P = C.P, tt = C.tt;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const gridA = st.lineU * tt;
    const turnedA = 1 - tt;
    const rhythm = M.form === 'rhythm';
    ctx.lineCap = 'round';

    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, PLOT_H); ctx.clip(); }

    /* the back wall's grid, which is the chart's grid once the cloud lines up */
    if (gridA > 0.004) {
      ctx.globalAlpha = gridA;
      for (const l of M.yLabels) {
        if (Math.abs(l.u + 1) < 1e-6 || (rhythm && Math.abs(l.u - 1) < 1e-6)) continue;
        dotted(ctx, P(-1, l.u, -1), P(1, l.u, -1), 5, 1.1, T.borderStrong);
      }
      if (!rhythm) for (const l of M.xLabels) {
        if (Math.abs(l.u + 1) < 1e-6) continue;
        dotted(ctx, P(l.u, -1, -1), P(l.u, 1, -1), 5, 1.1, T.borderSubtle);
      }
      ctx.globalAlpha = 1;
    }

    /* the box: its far edges fade as it lines up, since they fold onto the face */
    if (turnedA > 0.004) {
      ctx.globalAlpha = turnedA * 0.9;
      const EDGES = [[1, 3], [2, 3], [3, 7], [4, 5], [4, 6], [5, 7], [6, 7], [1, 5], [2, 6]];
      for (const [i, j] of EDGES) line(ctx, P(...CORNERS[i]), P(...CORNERS[j]), 0.75, T.border);
      /* the depth axis from the back corner, and the small ticks the turned box reads by */
      dotted(ctx, P(-1, -1, -1), P(-1, -1, 1), 4, 1, T.muted);
      ctx.globalAlpha = turnedA * 0.8;
      const tick = (a, b) => line(ctx, a, b, 0.75, T.muted);
      const ux6 = 6 / Math.max(1, C.kx * G.tx), uy6 = 6 / Math.max(1, C.ky * G.ty);
      for (const l of M.xLabels) tick(P(l.u, -1, -1), P(l.u, -1 - uy6, -1));
      for (const l of M.yLabels) tick(P(-1, l.u, -1), P(-1 - ux6, l.u, -1));
      if (rhythm && M.weeks > 1) for (let w = 0; w < M.weeks; w += 4) { const uz = -1 + 2 * w / (M.weeks - 1); tick(P(-1, -1, uz), P(-1 - ux6, -1, uz)); }
      ctx.globalAlpha = 1;
    }
    /* the floor and the up axis from the back corner: a lined-up chart's floor and its y axis, in series' grammar */
    ctx.fillStyle = T.borderStrong;
    line(ctx, P(-1, -1, -1), P(1, -1, -1), 1, T.borderStrong);
    const top = P(-1, 1, -1), foot = P(-1, -1, -1);
    dotted(ctx, foot, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); ctx.arc(top[0], top[1], END_R, 0, TAU); ctx.moveTo(foot[0] + END_R, foot[1]); ctx.arc(foot[0], foot[1], END_R, 0, TAU); ctx.fill();

    /* once it lines up: the rhythm's columns draw up from the evening, one weekday after another */
    if (rhythm && st.lineU > 0) {
      for (let d = 0; d < 7; d++) {
        const u = kit.ease.out(kit.clamp((st.lineU - d * 0.05) / 0.7, 0, 1));
        if (u <= 0) continue;
        const ux = -1 + (2 / 7) * (d + 0.5);
        ctx.globalAlpha = 0.55;
        dotted(ctx, P(ux, -1, 0), P(ux, -1 + 2 * u, 0), 3.2, 0.9, T.muted);
      }
      ctx.globalAlpha = 1;
    }
    ctx.restore();

    /* the dots, back to front, sized and inked by depth until the cloud lines up */
    const flatR = rhythm ? 2.9 : 2.6, flatA = rhythm ? 0.5 : 0.85;
    const jf = st.jit;
    const dimA = hover !== null ? kit.lerp(1, 0.5, dim.x) : 1;
    const col = T.rgb.s0;
    const drawn = [];
    M.pts.forEach((p, j) => {
      const g = growth(j);
      if (g <= 0) return;
      const s = P(p.ux + p.jx * jf, p.uy, p.uz);
      const f = kit.clamp((s[2] + 1) / 2, 0, 1);
      const r = kit.lerp(kit.lerp(1.5, 3.1, f), flatR, tt) * kit.clamp(g * 3, 0, 1);
      const a = kit.lerp(kit.lerp(0.42, 1, f), flatA, tt);
      drawn.push({ j, x: s[0], y: s[1], z: s[2], r, a });
    });
    drawn.sort((m, n) => m.z - n.z);
    for (const d of drawn) {
      ctx.globalAlpha = d.a * (hover !== null && hover !== d.j ? dimA : 1);
      ctx.fillStyle = kit.rgba(col);
      ctx.beginPath(); ctx.arc(d.x, d.y, d.r, 0, TAU); ctx.fill();
    }
    ctx.globalAlpha = 1;
    proj = drawn;

    /* the relation's line, drawn through the days from left to right once it lines up, with its spread dotted */
    if (!rhythm && M.fit && st.lineU > 0) {
      const u = kit.ease.out(st.lineU);
      const f = M.fit, x0 = f.lo, x1 = kit.lerp(f.lo, f.hi, u);
      const seg = (off) => {
        const a = P(M.ux(x0), M.uy(f.icept + f.slope * x0 + off), 0), b = P(M.ux(x1), M.uy(f.icept + f.slope * x1 + off), 0);
        return [a, b];
      };
      ctx.save();
      const b = G.b;
      if (tt > 0.5) { ctx.beginPath(); ctx.rect(b.left, b.top - 1, b.right - b.left, b.bottom - b.top + 2); ctx.clip(); }
      if (f.spread > 0) {
        ctx.globalAlpha = 0.5 * u;
        for (const off of [f.spread, -f.spread]) { const [a, c] = seg(off); dotted(ctx, a, c, 4, 1, T.ink); }
      }
      ctx.globalAlpha = 0.8;
      ctx.lineCap = 'round';
      const [a, c] = seg(0);
      line(ctx, a, c, 1.25, T.ink);
      ctx.restore();
      ctx.globalAlpha = 1;
    }

    /* a ring grows on the dot you rest on */
    if (hover !== null) {
      const d = drawn.find((x) => x.j === hover);
      if (d) {
        ctx.strokeStyle = kit.rgba(col, Math.min(1, col[3]) * 0.9);
        ctx.lineWidth = 1;
        ctx.beginPath(); ctx.arc(d.x, d.y, d.r + 1.5 + 3.5 * ring.x, 0, TAU); ctx.stroke();
      }
    }

    /* the lock: four brackets close on the face as the cloud lines up, then let go */
    if (st.engaged && !reduced()) {
      const since = clock - st.engT, ba = kit.clamp(since / 0.1, 0, 1) * (1 - kit.clamp((since - 0.9) / 0.4, 0, 1));
      if (ba > 0) {
        const sc = 1.1 - 0.1 * kit.ease.out(kit.clamp(since / 0.45, 0, 1));
        const f = [P(-1, -1, 1), P(-1, 1, 1), P(1, 1, 1), P(1, -1, 1)];
        const mx = (f[0][0] + f[2][0]) / 2, my = (f[0][1] + f[2][1]) / 2, arm = 9;
        ctx.strokeStyle = T.primary; ctx.lineWidth = 1; ctx.globalAlpha = ba;
        ctx.beginPath();
        for (const p of f) {
          const x = mx + (p[0] - mx) * sc * 1.03, y = my + (p[1] - my) * sc * 1.05, sx = Math.sign(x - mx), sy = Math.sign(y - my);
          ctx.moveTo(x - sx * arm, y); ctx.lineTo(x, y); ctx.lineTo(x, y - sy * arm);
        }
        ctx.stroke();
        ctx.globalAlpha = 1;
      }
    }

    /* labels: the chart's own ticks arrive with the line; the axis names belong to the turned box */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, PLOT_H); ctx.clip(); }
    const tickA = gridA * labelA;
    if (tickA > 0.004) {
      ctx.globalAlpha = tickA;
      ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textBaseline = 'middle'; ctx.textAlign = 'right';
      for (const l of M.yLabels) { const p = P(-1, l.u, -1); ctx.fillText(l.text, p[0] - 7, p[1]); }
      ctx.textBaseline = 'alphabetic';
      const xl = M.xLabels;
      const keep = rhythm ? xl.map((_, k) => k) : kit.labelIndices(xl.length, Math.max(2, Math.floor((G.b.right - G.b.left) / 52)));
      keep.forEach((k, n) => {
        const p = P(xl[k].u, -1, -1);
        ctx.textAlign = rhythm ? 'center' : n === keep.length - 1 && keep.length > 1 && p[0] > width - 30 ? 'right' : 'center';
        ctx.fillText(xl[k].text, p[0], PLOT_H - 7 + (p[1] - G.b.bottom));
      });
      ctx.globalAlpha = 1;
    }
    const nameA = turnedA * labelA;
    if (nameA > 0.004) {
      const o = P(-1, -1, -1);
      const name = (end, text) => {
        const len = Math.hypot(end[0] - o[0], end[1] - o[1]);
        const a = kit.clamp((len - 24) / 50, 0, 1) * nameA;
        if (a <= 0 || !text) return;
        const dx = (end[0] - o[0]) / (len || 1), dy = (end[1] - o[1]) / (len || 1);
        plate(ctx, text, end[0] + dx * 10, end[1] + dy * 10 + 3, dx > 0.3 ? 'left' : dx < -0.3 ? 'right' : 'center', 0.95 * a, T);
      };
      if (rhythm) { name(P(1, -1, -1), 'Weekday'); name(P(-1, 1, -1), 'Hour'); }
      else { name(P(1, -1, -1), kit.columnLabel(M.A)); name(P(-1, 1, -1), kit.columnLabel(M.B)); }
      name(P(-1, -1, 1), M.depthText);
    }
    ctx.restore();
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    if (!M.form) { kit.setReadout(parts.readout, [{ label: 'There is nothing here to place in a cloud' }]); return; }
    if (hover === null || !M.pts[hover]) {
      if (M.form === 'relation') {
        kit.setReadout(parts.readout, [{ label: 'Across', value: kit.columnLabel(M.A) }, { label: 'Up', value: kit.columnLabel(M.B) }]);
        /* in a narrow column the two names keep their room, and arrows stand for across and up */
        const cut = (e) => e.scrollWidth > e.clientWidth + 1;
        if (cut(parts.readout) || [...parts.readout.querySelectorAll('.vk-lab')].some(cut)) {
          kit.setReadout(parts.readout, [{ label: '→', value: kit.columnLabel(M.A) }, { label: '↑', value: kit.columnLabel(M.B) }]);
        }
      } else kit.setReadout(parts.readout, [{ when: M.depthText }, { label: kit.columnLabel(M.col) }]);
      return;
    }
    const p = M.pts[hover];
    if (M.form === 'relation') {
      const when = M.dates ? dayText(p.t) : M.key ? kit.formatCell(p.k) : '';
      const parts2 = [{ label: kit.columnLabel(M.A), value: kit.formatNumber(p.a) }, { label: kit.columnLabel(M.B), value: kit.formatNumber(p.b) }];
      kit.setReadout(parts.readout, when ? [{ when }].concat(parts2) : parts2);
    } else kit.setReadout(parts.readout, [{ when: dayText(p.t) }, { label: '', value: timeText(p.t) }]);
  }
  function setHover(j, from) {
    if (j === hover) { hoverFrom = from; return; }
    hover = j; hoverFrom = from;
    if (j !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: it runs while the cloud turns, lines up, rings grow, or dots arrive ---------- */
  function step(dt) {
    clock += dt;
    const still = reduced();
    let moving = false;
    if (st.tween) {
      st.tween.t += dt;
      const u = still ? 1 : kit.ease.inOut(kit.clamp(st.tween.t / st.tween.dur, 0, 1));
      st.e.forEach((s, i) => { s.x = kit.lerp(st.tween.from[i], st.tween.to[i], u); s.v = 0; });
      const home = st.tween.to.every((v) => v === 0);
      if (home && !st.engaged && offOf(eD()) < ENGAGE) engage(true);
      if (u >= 1) { if (!st.engaged) st.raw = eD(); st.tween = null; }
      moving = true;
    } else {
      if (st.coast) {
        if (still) st.coast = null;
        else {
          const f = Math.pow(0.88, dt * 60);
          turnBy(st.coast.wy * dt, st.coast.wx * dt);
          if (st.coast) { st.coast.wy *= f; st.coast.wx *= f; if (Math.hypot(st.coast.wy, st.coast.wx) < 0.05 || st.engaged) st.coast = null; }
          moving = true;
        }
      }
      const target = st.engaged ? [0, 0, 0] : st.raw;
      const d = st.engaged ? 0.35 : st.drag || st.coast ? 0.12 : 0.16, b = st.engaged ? 0.12 : 0;
      st.e.forEach((s, i) => { if (still) { s.x = target[i]; s.v = 0; } else kit.springTo(s, target[i], dt, d, b); });
      if (st.e.some((s, i) => s.x !== target[i] || s.v !== 0)) moving = true;
    }
    const l0 = st.lineU, j0 = st.jit;
    if (still) { st.lineU = st.engaged ? 1 : 0; st.jit = st.engaged ? 0 : 1; }
    else {
      st.lineU = st.engaged ? Math.min(1, st.lineU + dt / 0.32) : Math.max(0, st.lineU - dt / 0.2);
      st.jit = st.engaged ? Math.max(0, st.jit - dt / 0.4) : Math.min(1, st.jit + dt / 0.3);
    }
    if (st.lineU !== l0 || st.jit !== j0) moving = true;
    if (!still && st.engaged && clock - st.engT < 1.35) moving = true;
    if (still) { ring.x = 1; ring.v = 0; dim.x = hover !== null ? 1 : 0; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, hover !== null ? 1 : 0, dt, 0.2, 0);
      if (ring.x !== 1 || dim.x !== (hover !== null ? 1 : 0)) moving = true;
    }
    if (mode === 'live' && !still) {
      if (clock - revealAt < 0.35 || reveal < Infinity) moving = true;
      for (let i = 0; i < born.length; i++) if (born[i] !== undefined && clock - born[i] < 1.2) { moving = true; break; }
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives: dots arrive as the frame's sweep passes them ---------- */
  function markBorn() {
    if (!width || !M.form) return;
    const G = geom(), C = camera(G);
    M.pts.forEach((p, j) => {
      if (born[j] !== undefined) return;
      const s = C.P(p.ux + p.jx * st.jit, p.uy, p.uz);
      if (reveal === Infinity || s[0] <= reveal) born[j] = clock;
    });
  }
  function rowsWait(on) {
    for (const node of [parts.readout, parts.foot, say]) {
      if (!node) continue;
      node.classList.toggle('vk-fade', !on && !reduced());
      node.classList.toggle('vk-wait', on);
    }
  }
  function setReveal(x) {
    if (mode === 'none' && x === Infinity) return;
    if (mode === 'none') { mode = 'live'; revealAt = clock; }
    reveal = x;
    markBorn();
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer: a mouse turns the cloud freely, a finger along its path; resting reads a dot ---------- */
  function nearest(x, y, radius) {
    let best = null, bd = radius;
    for (const d of proj) { const q = Math.hypot(d.x - x, d.y - y) - d.r; if (q < bd) { bd = q; best = d.j; } }
    return best;
  }
  const local = (e) => { const r = canvas.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top]; };
  const onDown = (e) => {
    if (e.button !== 0 || !M.form) return;
    const [x, y] = local(e);
    const touch = e.pointerType === 'touch';
    st.drag = { x, y, x0: x, y0: y, t: performance.now(), t0: performance.now(), wy: 0, wx: 0, touch, moved: 0, id: e.pointerId };
    st.tween = null; st.coast = null;
    if (touch) st.g = st.engaged ? 1 : kit.clamp(1 - angOf(eD()) / angOf(START), -0.6, 1);
    if (!touch) { e.preventDefault(); try { parts.plot.setPointerCapture(e.pointerId); } catch (err) { /* capture is optional */ } }
    parts.plot.classList.add('vk-cloud-drag');
    loop.wake();
  };
  const onMove = (e) => {
    const [x, y] = local(e);
    const d = st.drag;
    if (!d || d.id !== e.pointerId) {
      if (e.pointerType === 'mouse' && !st.tween) setHover(nearest(x, y, 12), 'pointer');
      return;
    }
    const now = performance.now(), dts = Math.max(4, now - d.t) / 1000, dx = x - d.x, dy = y - d.y;
    d.moved = Math.max(d.moved, Math.hypot(x - d.x0, y - d.y0));
    d.x = x; d.y = y; d.t = now;
    if (d.moved < 3) return;
    if (hoverFrom === 'pointer' || hoverFrom === 'tap') setHover(null, null);
    if (d.touch) {
      st.g = kit.clamp(st.g + dx / Math.max(160, width * 0.55), -0.6, 1);
      st.raw = START.map((v) => v * (1 - st.g));
      checkDetent();
    } else {
      d.wy = d.wy * 0.4 + (dx * DRAG / dts) * 0.6;
      d.wx = d.wx * 0.4 + (dy * DRAG / dts) * 0.6;
      turnBy(dx * DRAG, dy * DRAG);
    }
    loop.wake();
  };
  const onUp = (e) => {
    const d = st.drag;
    if (!d || d.id !== e.pointerId) return;
    st.drag = null;
    parts.plot.classList.remove('vk-cloud-drag');
    try { parts.plot.releasePointerCapture(e.pointerId); } catch (err) { /* it may not be captured */ }
    const quick = d.moved < 5 && performance.now() - d.t0 < 450;
    if (quick && e.type === 'pointerup') {
      const [x, y] = local(e);
      const j = nearest(x, y, d.touch ? 20 : 12);
      if (d.touch || j !== null) setHover(j, j === null ? null : 'tap');
    } else if (st.engaged) st.raw = [0, 0, 0];
    else if (!d.touch && !reduced() && performance.now() - d.t < 70 && Math.hypot(d.wy, d.wx) > 1) st.coast = { wy: d.wy, wx: d.wx };
    loop.wake();
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerup', onUp);
  parts.plot.addEventListener('pointercancel', onUp);
  parts.plot.addEventListener('pointerleave', onLeave);

  /* ---------- keys: the arrows turn it; lined up, left and right read the dots across ---------- */
  const READ_KEYS = ['ArrowLeft', 'ArrowRight', 'Home', 'End', 'PageUp', 'PageDown'];
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey || !M.form) return;
    const k = e.key;
    if (k === 'Escape') {
      if (hover === null) return;
      e.preventDefault(); setHover(null, null); return;
    }
    if (st.engaged && READ_KEYS.includes(k)) {
      e.preventDefault();
      const n = M.order.length, i = hover === null ? null : M.order.indexOf(hover);
      let j;
      if (k === 'ArrowRight') j = i === null ? 0 : Math.min(n - 1, i + 1);
      else if (k === 'ArrowLeft') j = i === null ? n - 1 : Math.max(0, i - 1);
      else if (k === 'Home') j = 0;
      else if (k === 'End') j = n - 1;
      else if (k === 'PageDown') j = Math.min(n - 1, (i === null ? -1 : i) + 10);
      else j = Math.max(0, (i === null ? n : i) - 10);
      setHover(M.order[j], 'key');
      return;
    }
    const turn = { ArrowLeft: [-KEY_STEP, 0], ArrowRight: [KEY_STEP, 0], ArrowUp: [0, -KEY_STEP], ArrowDown: [0, KEY_STEP] }[k];
    if (!turn) return;
    e.preventDefault();
    st.tween = null; st.coast = null;
    turnBy(turn[0], turn[1]);
    loop.wake();
  };
  /* reaching the figure with the keyboard lines it up, so the arrows read it straight away */
  const onFocus = () => {
    let keyboard = false;
    try { keyboard = parts.plot.matches(':focus-visible'); } catch (err) { keyboard = false; }
    if (keyboard && !st.engaged && !st.drag) alignNow();
  };
  const onBlur = () => { if (hoverFrom === 'key') setHover(null, null); };
  parts.plot.addEventListener('keydown', onKey);
  parts.plot.addEventListener('focus', onFocus);
  parts.plot.addEventListener('blur', onBlur);

  /* ---------- setup and the handle ---------- */
  function placeTurn() {
    if (parts.foot) { turnBtn.classList.remove('vk-cloud-top'); parts.foot.appendChild(turnBtn); }
    else { turnBtn.classList.add('vk-cloud-top'); parts.root.appendChild(turnBtn); if (parts.readout) parts.readout.style.paddingRight = '96px'; }
    turnBtn.hidden = !M.form;
  }
  function apply() {
    M = model(out);
    born = [];
    hover = null; hoverFrom = null;
    writeFinding();
    kit.setLabel(parts.plot, ariaText(M, st.engaged));
    readout();
    kit.foot(parts.foot, out, opts);
    placeTurn();
    if (width) fitSay();
    if (mode === 'live') { markBorn(); rowsWait(reveal < Infinity); }
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    fitSay();
    if (hover === null) readout();
    if (mode === 'live') markBorn();
    draw();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; draw(); }
  });
  const stopDpr = kit.watchDpr(() => draw());
  const stopMotion = kit.watchReducedMotion(() => {
    if (reduced() && !st.engaged) alignNow();
    rowsWait(mode === 'live' && reveal < Infinity);
    loop.wake();
  });
  if (reduced()) { st.e.forEach((s) => { s.x = 0; }); st.raw = [0, 0, 0]; st.lineU = 1; st.jit = 0; }
  apply();
  if (reduced()) engage(true, true);
  if (mode === 'live') { revealAt = clock; loop.wake(); }

  return {
    plot: parts.plot,
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      out = next || {};
      apply();
      if (reduced() && !st.engaged) alignNow();
    },
    setReveal,
    destroy() {
      loop.stop(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      cancelAnimationFrame(wordsQuiet);
      turnBtn.removeEventListener('click', onTurn);
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerup', onUp);
      parts.plot.removeEventListener('pointercancel', onUp);
      parts.plot.removeEventListener('pointerleave', onLeave);
      parts.plot.removeEventListener('keydown', onKey);
      parts.plot.removeEventListener('focus', onFocus);
      parts.plot.removeEventListener('blur', onBlur);
      root.remove();
    },
  };
}
