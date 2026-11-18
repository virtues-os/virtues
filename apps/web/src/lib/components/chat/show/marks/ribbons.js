// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/ribbons.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* ribbons.js: the state timeline. One thin strip for each day, from midnight to midnight, shaded by which state held
   at each moment.

   A chart mark, `mark: "states"`, for any state that has a start and an end and changes through the day: a place,
   an activity, asleep or awake, busy or free. A week or a month of strips stacked one under the other shows how the
   days are built: a state that comes at the same hour lines up as a column, and a day that went differently stands
   out of the stack.
   - Four columns, found by name: day (a date), start and end (times: a timestamp, or a time of day such as 18:00),
     and state (a short name). what, place and label work for state, and date, started_at and ended_at for the
     others; a column not found by name is taken by position. Up to 600 rows and 6 states. A row is drawn on its
     day's strip from its start to its end, held to that day's midnight at each end, so a state that runs past
     midnight is two rows, one on each day.
   - Every day from the first to the last has a strip, so a day with no rows is a bare track. An hour with no row is
     the bare track too: a fine dotted line through the strip, left out when strips are under 5px apart, where it
     would read as a texture.
   - The x axis is the hour of the day, midnight to midnight, the same for every strip. Each day is labelled at the
     left, every day when the strips are tall enough and each Monday when they are not, and a week starts after a
     small gap.
   - Up to six states, told apart by the accent, ink weights and halftones. The state with the most hours is the
     ground of the days and takes the quietest fill, a pale ink tint, and the state with the fewest takes the
     strongest, the solid accent, so the rare stretches are the ones the eye finds. In between, from quiet to
     strong: a coarse halftone of subtle ink, a fine ink halftone, an accent halftone, and solid ink at 60%. With
     fewer states the steps in between are left out, so two states are tint and accent. A 1px gap of the page
     separates two stretches.
   - A legend under the plot names the states, largest first, on up to two lines. Resting on a name lifts its
     stretches and dims the rest.
   - Rest on a stretch, or tab to the strips and use the arrow keys (left and right along a day, up and down between
     days, Page Up and Page Down by a week), to read the day, the state and its hours, such as "Thu, Nov 12  The
     boatyard  5:57 PM to 7:37 PM". The stretches of that state stay bright, the rest dim, and a hairline ring marks
     the stretch.
   - No totals, counts or percentages are drawn. The figure's label says when each state usually falls, in words.
   Block: readout 26 + plot 200 + legend 40 + foot 26 = 292px, whatever the rows, so the frame holds it before they
   arrive. In the frame each stretch opens from its track as the sweep passes its start; under reduced motion the
   strips are there at once.

   Contract: see the top of kit.js. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'ribbons', kinds: ['chart'], marks: ['states'] });

const PLOT_H = 200;
const PAD = Object.freeze({ top: 6, right: 8, bottom: 22, left: 44 });
const LEGEND_H = 40;           /* 4px above it, then two 18px lines */
const MAX_STATES = 6;
const MAX_PITCH = 16;          /* a day's row is never taller than this, so three days do not look like bars */
const MIN_PITCH = 1;           /* past 172 days a strip would be under a pixel, so only the latest days are drawn */
const GROW = 0.42, GROW_BOUNCE = 0.12;
const TAU = Math.PI * 2;
const DAY_MS = 864e5;
const HINT = 'Use the left and right arrow keys to read each stretch of a day, up and down to move between days, and Page Up and Page Down to move by a week.';

/* The fills, from the quietest to the strongest. A screen is a square dot grid, as stack.js draws its halftones. */
const STYLES = Object.freeze({
  tint: Object.freeze({ fill: 'solid', color: 'ink', alpha: 0.1 }),
  subtleDots: Object.freeze({ fill: 'screen', color: 'subtle', r: 0.8, pitch: 3.5 }),
  inkDots: Object.freeze({ fill: 'screen', color: 'ink', r: 0.55, pitch: 2.5 }),
  accentDots: Object.freeze({ fill: 'screen', color: 'primary', r: 0.8, pitch: 2.5 }),
  ink: Object.freeze({ fill: 'solid', color: 'ink', alpha: 0.6 }),
  accent: Object.freeze({ fill: 'solid', color: 'primary', alpha: 1 }),
});
/* Which fills N states take, largest state first. */
const LADDER = Object.freeze([
  [],
  ['accent'],
  ['tint', 'accent'],
  ['tint', 'ink', 'accent'],
  ['tint', 'inkDots', 'ink', 'accent'],
  ['tint', 'inkDots', 'accentDots', 'ink', 'accent'],
  ['tint', 'subtleDots', 'inkDots', 'accentDots', 'ink', 'accent'],
]);

const CSS = `
.vk-ribbons .vk-plot{user-select:none;-webkit-user-select:none}
.vk-ribbons .vk-rb-legend{display:flex;flex-wrap:wrap;align-content:flex-start;gap:0 16px;height:36px;margin:4px 0 0;overflow:hidden;font-size:12px;line-height:18px;color:var(--vk-muted)}
.vk-ribbons .vk-rb-key{display:inline-flex;align-items:center;gap:6px;white-space:nowrap;cursor:default;transition:color .15s cubic-bezier(.2,0,0,1)}
.vk-ribbons .vk-rb-key.vk-on{color:var(--vk-ink)}
@media (prefers-reduced-motion:reduce){.vk-ribbons .vk-rb-key{transition:none}}
`;

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + PLOT_H + LEGEND_H + (options.foot === false ? 0 : kit.ROWS.foot);
}

/* The strips' rect: the hours run along it and the frame's dot runs its floor, the hour axis. */
export function box(width) {
  return kit.plotBox(width, PLOT_H, PAD);
}

/* ---------- words ---------- */
function formatter(opts) {
  try { return new Intl.DateTimeFormat(undefined, opts); } catch (err) { return null; }
}
const F = {
  dayShort: formatter({ weekday: 'short', month: 'short', day: 'numeric' }),
  monthDay: formatter({ month: 'short', day: 'numeric' }),
  weekday: formatter({ weekday: 'short' }),
  time: formatter({ hour: 'numeric', minute: '2-digit' }),
  hour: formatter({ hour: 'numeric' }),
};
const fmt = (f, t, fallback) => (f ? f.format(t) : fallback(t));
const dayShort = (t) => fmt(F.dayShort, t, (x) => kit.formatTime(x, { withDate: true, withTime: false }));
const monthDay = (t) => fmt(F.monthDay, t, (x) => kit.formatTime(x, { withDate: true, withTime: false }));
const rowLabel = (t) => `${fmt(F.weekday, t, (x) => ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'][x.getDay()])} ${t.getDate()}`;
function hourText(h) {
  const t = new Date(2000, 0, 3, Math.floor(((h % 24) + 24) % 24));
  return fmt(F.hour, t, (x) => kit.formatTime(x));
}
/* 18.5 -> "6:30 PM", 18 -> "6 PM", 0 and 24 -> "midnight" */
function clockText(h) {
  if (h <= 0 || h >= 24) return 'midnight';
  let hh = Math.floor(h), mm = Math.round((h - hh) * 60);
  if (mm === 60) { hh += 1; mm = 0; }
  if (hh >= 24) return 'midnight';
  const t = new Date(2000, 0, 3, hh, mm);
  return mm ? fmt(F.time, t, (x) => kit.formatTime(x)) : fmt(F.hour, t, (x) => kit.formatTime(x));
}
const spanText = (a, b) => `${clockText(a)} to ${clockText(b)}`;
function rangeText(a, b) {
  if (a.getTime() === b.getTime()) return dayShort(a);
  if (a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth()) return `${monthDay(a)} to ${b.getDate()}`;
  return `${monthDay(a)} to ${monthDay(b)}`;
}

/* When a state usually falls, for the figure's label: the parts of the day that hold at least a quarter of its hours. */
const PARTS = Object.freeze([
  ['at night', 22, 29], ['in the morning', 5, 12], ['around midday', 12, 14], ['in the afternoon', 14, 17], ['in the evening', 17, 22],
]);
function usually(segs) {
  const hours = PARTS.map(() => 0);
  let total = 0;
  for (const s of segs) {
    PARTS.forEach(([, a, b], i) => {
      for (const off of [0, -24]) hours[i] += Math.max(0, Math.min(s.h1, b + off) - Math.max(s.h0, a + off));
    });
    total += s.h1 - s.h0;
  }
  if (!total) return '';
  const ranked = PARTS.map((p, i) => [p[0], hours[i] / total]).filter((p) => p[1] >= 0.25).sort((x, y) => y[1] - x[1]);
  if (!ranked.length) return 'at many hours';
  if (ranked[0][1] >= 0.75 || ranked.length === 1) return `mostly ${ranked[0][0]}`;
  return `${ranked[0][0]} and ${ranked[1][0]}`;
}

/* ---------- the model ---------- */
const EPOCH = new Date(2000, 0, 3).getTime();          /* a Monday, so (index mod 7) is the weekday with Monday 0 */
const dayIndex = (t) => Math.round((new Date(t.getFullYear(), t.getMonth(), t.getDate()).getTime() - EPOCH) / DAY_MS);
const dateOf = (d) => new Date(2000, 0, 3 + d);
const weekdayOf = (d) => ((d % 7) + 7) % 7;
const CLOCK = /^(\d{1,2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?$/;

/* A time as hours into day d: a timestamp is read on the reader's clock and held to the day's midnights; a bare
   time of day is hours from that day's midnight. Null when it is neither. */
function hoursInto(v, d) {
  if (typeof v !== 'string') return null;
  const m = v.trim().match(CLOCK);
  if (m) {
    const h = Number(m[1]) + Number(m[2]) / 60 + Number(m[3] || 0) / 3600;
    return h >= 0 && h <= 24 ? h : null;
  }
  const t = kit.parseTime(v);
  if (!t) return null;
  const td = dayIndex(t);
  if (td < d) return 0;
  if (td > d) return 24;
  return t.getHours() + t.getMinutes() / 60 + t.getSeconds() / 3600;
}

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const lower = columns.map((c) => (typeof c === 'string' ? c.trim().toLowerCase() : ''));
  const named = (names, at) => { const i = lower.findIndex((c) => names.includes(c)); return columns[i >= 0 ? i : at]; };
  const dayCol = named(['day', 'date'], 0), startCol = named(['start', 'started_at'], 1);
  const endCol = named(['end', 'ended_at'], 2), stateCol = named(['state', 'what', 'place', 'label'], 3);
  const segs = [];
  rows.forEach((row, i) => {
    if (!row || typeof row !== 'object') return;
    const dt = kit.parseTime(row[dayCol]);
    const state = row[stateCol] === null || row[stateCol] === undefined ? '' : String(row[stateCol]).trim();
    if (!dt || !state) return;
    const d = dayIndex(dt);
    const h0 = hoursInto(row[startCol], d);
    let h1 = hoursInto(row[endCol], d);
    if (h0 === null || h1 === null) return;
    /* a bare time of day that ends at or before its start runs to midnight */
    if (h1 <= h0 && CLOCK.test(String(row[endCol]).trim())) h1 = 24;
    const a = kit.clamp(h0, 0, 24), b = kit.clamp(h1, 0, 24);
    if (b - a < 1 / 120) return;
    segs.push({ i, row, d, h0: a, h1: b, state });
  });
  const M = { segs: [], days: [], kinds: [], byDay: new Map(), count: 0, cut: false };
  if (!segs.length) return M;

  /* states by their hours, largest first; past six the smallest are left out (show.rs refuses a seventh) */
  const hours = new Map();
  for (const s of segs) hours.set(s.state, (hours.get(s.state) || 0) + (s.h1 - s.h0));
  const names = [...hours.keys()].sort((a, b) => hours.get(b) - hours.get(a) || a.localeCompare(b)).slice(0, MAX_STATES);
  const ladder = LADDER[names.length];
  M.kinds = names.map((name, k) => ({ k, name, hours: hours.get(name), style: ladder[k], segs: [] }));
  const kindOf = new Map(M.kinds.map((kd) => [kd.name, kd]));

  let d0 = Math.min(...segs.map((s) => s.d));
  const d1 = Math.max(...segs.map((s) => s.d));
  const most = Math.floor((PLOT_H - PAD.top - PAD.bottom - 4) / MIN_PITCH);
  if (d1 - d0 + 1 > most) { d0 = d1 - most + 1; M.cut = true; }
  for (let d = d0; d <= d1; d++) { M.days.push(d); M.byDay.set(d, []); }
  for (const s of segs) {
    const kd = kindOf.get(s.state);
    if (!kd || s.d < d0) continue;
    s.kind = kd.k;
    kd.segs.push(s);
    M.byDay.get(s.d).push(s);
  }
  /* each day's stretches in time order; the order the keys walk */
  for (const d of M.days) {
    const list = M.byDay.get(d);
    list.sort((a, b) => a.h0 - b.h0 || a.h1 - b.h1);
    for (const s of list) { s.at = M.segs.length; M.segs.push(s); }
  }
  M.count = M.segs.length;
  M.first = dateOf(M.days[0]);
  M.last = dateOf(M.days[M.days.length - 1]);
  return M;
}

function ariaLabel(M) {
  if (!M.count) return 'State timeline with no rows';
  const n = M.days.length;
  const span = n === 1 ? dayShort(M.first) : `${n} days from ${dayShort(M.first)} to ${dayShort(M.last)}`;
  const kinds = M.kinds.map((kd) => { const u = usually(kd.segs); return u ? `${kd.name}, ${u}` : kd.name; }).join('; ');
  return `State timeline for ${span}, one strip a day from midnight to midnight, shaded by which state held at each hour. ${kinds}.`;
}

/* The legend's and the readout's mark for a fill, drawn the way the strip draws it. */
function styleSwatch(name) {
  const st = STYLES[name];
  const c = st.alpha && st.alpha < 1 ? `color-mix(in srgb, var(--vk-${st.color}) ${Math.round(st.alpha * 100)}%, var(--vk-page))` : `var(--vk-${st.color})`;
  let body;
  if (st.fill === 'solid') body = `<rect x="0" y="0" width="14" height="8" style="fill:${c}"/>`;
  else {
    const dots = [];
    for (let y = 1.25; y < 8; y += st.pitch) for (let x = 1.25; x < 14; x += st.pitch) dots.push(`<circle cx="${x}" cy="${y}" r="${st.r}"/>`);
    body = `<g style="fill:${c}">${dots.join('')}</g>`;
  }
  return `<svg class="vk-sw" width="14" height="8" viewBox="0 0 14 8" aria-hidden="true">${body}</svg>`;
}

/* ---------- the component ---------- */
export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('ribbons', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-ribbons');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: PLOT_H, readout: true, foot: opts.foot !== false, hint: HINT });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);
  const legend = kit.h('div', 'vk-rb-legend');
  parts.root.insertBefore(legend, parts.foot || null);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;        /* hover: a stretch's index in M.segs */
  let keyKind = null;                        /* the state lifted from the legend */
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  let screens = new Map(), screensKey = '';
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const B = () => box(width);
  const X = (h) => { const b = B(); return b.left + (h / 24) * (b.right - b.left); };
  let rowsGeo = null, rowsKey = '';
  /* Each day's strip: its top and its height, snapped to device pixels. A week starts after a small gap. */
  function rows() {
    const k = canvas.width / Math.max(1, width) || kit.dpr();
    const key = `${width}|${k}|${M.days.length}|${M.days[0]}`;
    if (rowsGeo && rowsKey === key) return rowsGeo;
    const b = B(), n = M.days.length;
    const avail = b.bottom - b.top - 4;
    const mondays = M.days.filter((d, i) => i > 0 && weekdayOf(d) === 0).length;
    const weekGap = n > 7 && n <= 42 ? 3 : 0;
    const pitch = Math.min(MAX_PITCH, (avail - weekGap * mondays) / Math.max(1, n));
    const stripH = pitch >= 8 ? pitch - 3 : pitch >= 4 ? pitch - 1.5 : Math.max(1 / k, pitch - 0.5);
    const snap = (v) => Math.round(v * k) / k;
    const out = new Map();
    let y = b.top;
    M.days.forEach((d, i) => {
      if (i > 0 && weekdayOf(d) === 0) y += weekGap;
      const top = snap(y + (pitch - stripH) / 2);
      out.set(d, { i, top, h: Math.max(1 / k, snap(stripH)), mid: top + snap(stripH) / 2, y0: y, y1: y + pitch });
      y += pitch;
    });
    rowsGeo = { map: out, pitch, stripH, k };
    rowsKey = key;
    return rowsGeo;
  }
  function segRect(s) {
    const R = rows(), r = R.map.get(s.d), k = R.k;
    const snap = (v) => Math.round(v * k) / k;
    /* a 1px gap of the page where two stretches meet */
    const x0 = snap(X(s.h0) + (s.h0 > 0 ? 0.5 : 0)), x1 = snap(X(s.h1) - (s.h1 < 24 ? 0.5 : 0));
    return { x0, x1: Math.max(x0 + 1 / k, x1), top: r.top, h: r.h, mid: r.mid };
  }

  /* ---------- the fills ---------- */
  /* A halftone as a canvas pattern, one dot per tile in device pixels, so it stays crisp at any density. */
  function screen(ctx, st, k) {
    const key = `${k}|${tokens.signature}`;
    if (key !== screensKey) { screens = new Map(); screensKey = key; }
    const id = `${st.color}|${st.r}|${st.pitch}`;
    let s = screens.get(id);
    if (!s) {
      const pd = Math.max(2, Math.round(st.pitch * k));
      const c = document.createElement('canvas');
      c.width = pd; c.height = pd;
      const g = c.getContext('2d');
      g.fillStyle = kit.rgba(tokens.rgb[st.color]);
      g.beginPath(); g.arc(pd / 2, pd / 2, Math.max(0.5, st.r * k), 0, TAU); g.fill();
      s = { pattern: ctx.createPattern(c, 'repeat'), pd };
      s.pattern.setTransform(new DOMMatrix([1 / k, 0, 0, 1 / k, 0, 0]));
      screens.set(id, s);
    }
    return s;
  }
  const solid = (st) => kit.rgba(kit.mix(tokens.rgb.page, tokens.rgb[st.color], st.alpha));

  function growth(s) {
    if (mode === 'none' || reduced()) return 1;
    const t = born[s.at];
    if (t === undefined) return 0;
    return Math.max(0, kit.stepSpring(clock - t, GROW, GROW_BOUNCE));
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, PLOT_H);
    const T = tokens, b = B(), L = b.left, Rt = b.right;
    if (!M.count) return;
    const R = rows(), k = R.k;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const lift = hover !== null ? M.segs[hover].kind : keyKind;
    const dimA = lift !== null ? kit.lerp(1, 0.32, dim.x) : 1;
    const every = [3, 6, 12].find((s) => (s * (Rt - L)) / 24 >= 52) || 12;

    /* the hour grid behind the strips, and the floor's hour dots and words */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, PLOT_H); ctx.clip(); }
    for (let h = every; h < 24; h += every) dotted(ctx, X(h), b.top, X(h), b.bottom - 3, 5, 1.1, T.borderSubtle);
    ctx.fillStyle = T.borderStrong;
    ctx.beginPath();
    for (let h = 0; h <= 24; h++) { const x = X(h); const r = h % every === 0 ? 1.2 : 0.8; ctx.moveTo(x + r, b.bottom); ctx.arc(x, b.bottom, r, 0, TAU); }
    ctx.fill();
    /* the bare track of every day, while a strip is tall enough for it to read as a line and not a texture */
    if (R.pitch >= 5) {
      ctx.strokeStyle = T.borderStrong; ctx.lineWidth = 1; ctx.lineCap = 'round'; ctx.setLineDash([0, 3]);
      ctx.beginPath();
      for (const r of R.map.values()) { ctx.moveTo(L + 1, r.mid); ctx.lineTo(Rt - 1, r.mid); }
      ctx.stroke();
      ctx.setLineDash([]);
    }
    ctx.restore();

    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textAlign = 'center'; ctx.textBaseline = 'alphabetic';
    for (let h = every; h < 24; h += every) ctx.fillText(hourText(h), X(h), PLOT_H - 6);
    /* the days: each one when there is room, else each Monday and the first */
    ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    const hoverDay = hover !== null ? M.segs[hover].d : null;
    const all = R.pitch >= 10;
    const hoverMid = hoverDay !== null ? R.map.get(hoverDay).mid : null;
    for (const [d, r] of R.map) {
      const first = r.i === 0;
      if (!all && !(weekdayOf(d) === 0 || first) && d !== hoverDay) continue;
      /* the day you rest on is labelled, and a label that would touch it gives way */
      if (!all && hoverMid !== null && d !== hoverDay && Math.abs(r.mid - hoverMid) < 11) continue;
      if (!all && first && weekdayOf(d) !== 0) {
        /* the first day is labelled only when the next Monday's label leaves it room */
        const next = M.days.find((x) => x > d && weekdayOf(x) === 0);
        if (next !== undefined && R.map.get(next).mid - r.mid < 11 && d !== hoverDay) continue;
      }
      ctx.fillStyle = d === hoverDay ? T.ink : T.muted;
      ctx.fillText(all ? rowLabel(dateOf(d)) : monthDay(dateOf(d)), L - 7, r.mid);
    }
    ctx.restore();

    /* the stretches: the page under them, so the track never shows through, then each state's fill */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, PLOT_H); ctx.clip(); }
    const byKind = M.kinds.map(() => []);
    const under = new Path2D();
    for (const s of M.segs) {
      const g = growth(s);
      if (g <= 0) continue;
      const q = segRect(s);
      const hh = Math.max(1 / k, q.h * (0.25 + 0.75 * Math.min(1, g)));
      const rr = { x0: q.x0, x1: q.x1, top: q.mid - hh / 2, h: hh };
      if (g < 1) { rr.top = Math.round(rr.top * k) / k; }
      under.rect(rr.x0 - 0.25, rr.top, rr.x1 - rr.x0 + 0.5, rr.h);
      byKind[s.kind].push(rr);
    }
    ctx.fillStyle = T.page;
    ctx.fill(under);
    M.kinds.forEach((kd, i) => {
      const st = STYLES[kd.style];
      const rects = byKind[i];
      if (!rects.length) return;
      ctx.globalAlpha = lift !== null && lift !== i ? dimA : 1;
      const p = new Path2D();
      for (const r of rects) p.rect(r.x0, r.top, r.x1 - r.x0, r.h);
      if (st.fill === 'solid') { ctx.fillStyle = solid(st); ctx.fill(p); }
      else { ctx.fillStyle = screen(ctx, st, k).pattern; ctx.fill(p); }
    });
    ctx.globalAlpha = 1;
    ctx.restore();

    /* the stretch you rest on: a hairline ring round it */
    if (hover !== null) {
      const q = segRect(M.segs[hover]);
      const grow = 1.5 + 1.5 * ring.x;
      ctx.save();
      ctx.strokeStyle = T.ink; ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.roundRect(q.x0 - grow, q.top - grow, q.x1 - q.x0 + grow * 2, q.h + grow * 2, Math.min(3, (q.h + grow * 2) / 2));
      ctx.stroke();
      ctx.restore();
    }
  }

  /* ---------- the readout and the legend ---------- */
  function readout() {
    if (!parts.readout) return;
    if (hover === null || !M.segs[hover]) {
      kit.setReadout(parts.readout, M.count ? [{ when: rangeText(M.first, M.last) }] : []);
      return;
    }
    const s = M.segs[hover], kd = M.kinds[s.kind];
    kit.setReadout(parts.readout, [{ when: dayShort(dateOf(s.d)) }, { swatch: styleSwatch(kd.style), label: kd.name, value: spanText(s.h0, s.h1) }]);
  }
  let legendKey = '';
  function writeLegend() {
    const key = M.kinds.map((kd) => `${kd.style}|${kd.name}`).join('\n');
    if (key === legendKey) return;
    legendKey = key;
    legend.textContent = '';
    M.kinds.forEach((kd, i) => {
      const item = kit.h('span', 'vk-rb-key');
      item.innerHTML = `${styleSwatch(kd.style)}<span>${kit.esc(kd.name)}</span>`;
      item.addEventListener('pointerenter', () => { if (hover === null) liftKind(i); });
      item.addEventListener('pointerleave', () => liftKind(null));
      legend.appendChild(item);
    });
  }
  function legendShown() {
    const lift = hover !== null ? M.segs[hover].kind : keyKind;
    [...legend.children].forEach((n, i) => n.classList.toggle('vk-on', lift === i));
  }
  function liftKind(i) {
    if (keyKind === i) return;
    keyKind = i;
    legendShown();
    loop.wake();
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    legendShown();
    loop.wake();
  }

  /* ---------- the loop: it runs while a ring grows, the dim moves, or stretches open ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    const lifted = hover !== null || keyKind !== null;
    if (reduced()) { ring.x = 1; ring.v = 0; dim.x = lifted ? 1 : 0; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, lifted ? 1 : 0, dt, 0.2, 0);
      moving = ring.x !== 1 || dim.x !== (lifted ? 1 : 0);
    }
    if (mode === 'live' && !reduced()) {
      if (clock - revealAt < 0.35 || reveal < Infinity) moving = true;
      for (let i = 0; i < born.length; i++) if (born[i] !== undefined && clock - born[i] < 1.2) { moving = true; break; }
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    if (!width) return;
    for (const s of M.segs) if (born[s.at] === undefined && (reveal === Infinity || X(s.h0) <= reveal)) born[s.at] = clock;
  }
  function rowsWait(on) {
    for (const node of [parts.readout, legend, parts.foot]) {
      if (!node) continue;
      node.classList.toggle('vk-fade', !on && !reduced());
      node.classList.toggle('vk-wait', on);
    }
  }
  function setReveal(x) {
    if (mode === 'none' && x === Infinity) return;
    if (mode === 'none') { mode = 'live'; revealAt = clock; }
    reveal = reduced() ? Infinity : x;
    markBorn();
    rowsWait(reveal < Infinity);
    /* drawn now as well as on the next frame, so the strips keep up with the frame's sweep */
    draw();
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  function pick(px, py) {
    if (!M.count) return null;
    const R = rows(), b = B();
    if (px < b.left - 4 || px > b.right + 4 || py > b.bottom + 2) return null;
    let day = null;
    for (const [d, r] of R.map) if (py >= r.y0 - 1.5 && py < r.y1 + 1.5) { day = d; if (py >= r.y0 && py < r.y1) break; }
    if (day === null) return null;
    const list = M.byDay.get(day);
    const h = ((px - b.left) / (b.right - b.left)) * 24;
    let best = null, bd = 6;
    for (const s of list) {
      const q = segRect(s);
      const dpx = px < q.x0 ? q.x0 - px : px > q.x1 ? px - q.x1 : 0;
      if (dpx < bd || (dpx === 0 && bd === 0 && s.h0 <= h)) { bd = dpx; best = s.at; }
    }
    return best;
  }
  const local = (e) => { const r = canvas.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top]; };
  const onMove = (e) => {
    if (!width || !M.count) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    const [x, y] = local(e);
    const i = pick(x, y);
    if (i === null) { if (hoverFrom === 'pointer') setHover(null, null); return; }
    setHover(i, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.count) return;
    const [x, y] = local(e);
    const i = pick(x, y);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === i) { setHover(null, null); return; }
    if (i !== null) setHover(i, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);

  /* the stretch on another day nearest the hours of this one, skipping days with none */
  function onDay(from, dir, by = 1) {
    const s = M.segs[from], mid = (s.h0 + s.h1) / 2;
    const at = M.days.indexOf(s.d);
    for (let j = at + dir * by; j >= 0 && j < M.days.length; j += dir) {
      const list = M.byDay.get(M.days[j]);
      if (!list.length) continue;
      let best = list[0], bd = Infinity;
      for (const o of list) {
        const d = o.h0 <= mid && o.h1 >= mid ? 0 : Math.min(Math.abs(o.h0 - mid), Math.abs(o.h1 - mid));
        if (d < bd) { bd = d; best = o; }
      }
      return best.at;
    }
    return from;
  }
  const onKey = (e) => {
    if (e.altKey || e.ctrlKey || e.metaKey || !M.count) return;
    const n = M.segs.length;
    let next;
    switch (e.key) {
      case 'ArrowRight': next = hover === null ? 0 : Math.min(n - 1, hover + 1); break;
      case 'ArrowLeft': next = hover === null ? 0 : Math.max(0, hover - 1); break;
      case 'ArrowDown': next = hover === null ? 0 : onDay(hover, 1); break;
      case 'ArrowUp': next = hover === null ? 0 : onDay(hover, -1); break;
      case 'PageDown': next = hover === null ? 0 : onDay(hover, 1, 7); break;
      case 'PageUp': next = hover === null ? 0 : onDay(hover, -1, 7); break;
      case 'Home': next = 0; break;
      case 'End': next = n - 1; break;
      case 'Escape':
        if (hover === null) return;
        e.preventDefault(); setHover(null, 'key'); return;
      default: return;
    }
    e.preventDefault();
    setHover(next, 'key');
  };
  const onBlur = () => { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); };
  parts.plot.addEventListener('keydown', onKey);
  parts.plot.addEventListener('blur', onBlur);

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    born = [];
    rowsGeo = null;
    hover = null; hoverFrom = null; keyKind = null;
    kit.setLabel(parts.plot, ariaLabel(M));
    writeLegend();
    legendShown();
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') { markBorn(); rowsWait(reveal < Infinity); }
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    rowsGeo = null;
    if (mode === 'live') markBorn();
    draw();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; draw(); }
  });
  const stopDpr = kit.watchDpr(() => { rowsGeo = null; draw(); });
  const stopMotion = kit.watchReducedMotion(() => { if (reduced() && reveal < Infinity) setReveal(Infinity); else { rowsWait(mode === 'live' && reveal < Infinity); loop.wake(); } });
  apply();
  if (mode === 'live') { revealAt = clock; if (reduced()) setReveal(Infinity); loop.wake(); }

  return {
    plot: parts.plot,
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      out = next || {};
      apply();
    },
    setReveal,
    destroy() {
      loop.stop(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointerleave', onLeave);
      parts.plot.removeEventListener('keydown', onKey);
      parts.plot.removeEventListener('blur', onBlur);
      root.remove();
    },
  };
}
