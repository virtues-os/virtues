// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/tripline.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* tripline.js: Itinerary strip map. kind "chart", mark "itinerary".

   An ordered run of stays drawn as a transit line, the way a train's door shows its line: each stop is a station as
   long as the time spent there, with a dot for each night, and the legs between stops carry how they were traveled.
   It fits any sequence of stays where the order and the length of each matter more than the geography: a trip, a
   season of travel, moves between homes.

   Each row is a stay, in order: a place, when it began and when it ended, and optionally how it was reached.
   - The line runs across the plot in the accent. Each stay is a station on it, a capsule whose length is its days,
     with a small dot inside it at each midnight of the stay, so a four-night stay holds four dots. The legs between
     stations share the rest of the line equally, and each carries the travel mode word the row gives.
   - The first row may have no start and the last no end: those are where the run leaves from and where it arrives,
     drawn as round ends of the line rather than sized, with the one time each has.
   - Above each station is its name in the serif. Under it are its dates and its nights. A name or a date that would
     run into its neighbor is set smaller, then shortened, and then left to the readout.
   - Times or bare dates both work. A stay that ends on a bare date left during that day, so its last night counts,
     and a leg between bare dates gives its day without a length. A run that crosses a new year writes the year in
     every date, and at its shortest only the month and year (Mar 2019 to Jul 2021).
   - Rest on a station, or tab to the chart and use the left and right arrow keys, which step through the stations
     and the legs between them: the others dim, a ring grows around it, and the readout gives the stay (Pinhão,
     June 16 to 18, 2 nights), the end (Home, Departed Jun 12, 8:00 AM) or the leg (Porto to Pinhão, Train, 2 h 30 m).
     Home and End jump to the ends, and Escape clears it.
   - In the frame the line draws itself from the left as the frame's sweep passes, which runs along the line itself,
     and each station opens as the line reaches it. Under reduced motion it is there at once. Nothing runs while
     nothing moves.

   Columns, found by name: place, started_at and ended_at (label, name, place_name, title, start, arrived_at, end and
   left_at work too), and an optional mode (or travel), only when a record names it. 1 to 30 rows, drawn in the
   query's order.

   Contract: see the top of kit.js. Block: readout 26 + plot 120 + foot 26 = 172px, whatever the stays. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'tripline', kinds: ['chart'], marks: ['itinerary'] });

const HEIGHT = 120;
const LINE_Y = 58;
const PAD = Object.freeze({ top: 8, right: 8, bottom: HEIGHT - LINE_Y, left: 8 });
const MAX_ROWS = 30;
const CAP_H = 12, END_R = 5.5, LINE_W = 2.75, NIGHT_R = 1.25;
const EDGE = 20;              /* room at each end of the line for an end's name */
const LEG_MIN = 26, LEG_MAX = 96;
const U_MAX = 70;             /* px for a day, at most, so a short run of stays does not stretch */
const MAX_DAYS = 45;          /* a stay longer than this is drawn this long */
const NAME_Y = LINE_Y - 20, MODE_Y = LINE_Y - 9, DATE_Y = LINE_Y + 19, NIGHT_Y = LINE_Y + 31;
const NAME_PX = Object.freeze([15, 13]);
const TAU = Math.PI * 2;
const DAY_MS = 864e5;
const UNNAMED = 'A place without a name';
const END_WORD = Object.freeze({ first: 'Departed', last: 'Arrived' });   /* under an end, and in its readout */
const HINT = 'Use the left and right arrow keys to step through the stays and the travel between them.';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

/* The plot's rect. Its bottom is the line, so the frame's dot runs where the line will draw. */
export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- words ---------- */
function formatter(opts) {
  try { return new Intl.DateTimeFormat(undefined, opts); } catch (err) { return null; }
}
const F = {
  md: formatter({ month: 'long', day: 'numeric' }), mdShort: formatter({ month: 'short', day: 'numeric' }),
  mdy: formatter({ month: 'long', day: 'numeric', year: 'numeric' }), mdyShort: formatter({ month: 'short', day: 'numeric', year: 'numeric' }),
  my: formatter({ month: 'short', year: 'numeric' }), time: formatter({ hour: 'numeric', minute: '2-digit' }),
};
const sameDay = (a, b) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
/* A day in words. `year` adds the year, for a run of stays that crosses one. */
const dayOf = (t, short, year) => {
  const f = year ? (short ? F.mdyShort : F.mdy) : (short ? F.mdShort : F.md);
  return f ? f.format(t) : `${kit.formatTime(t, { withDate: true, withTime: false })}${year ? `, ${t.getFullYear()}` : ''}`;
};
const monthOf = (t) => (F.my ? F.my.format(t) : `${kit.formatTime(t, { withDate: true, withTime: false })}, ${t.getFullYear()}`);
/* "June 12 to 16", "June 30 to July 2", or "12 to 16 June" where the day comes first. With `year`, the year follows
   the range when it is one year ("June 12 to 16, 2026") and each end when it is two ("July 16, 2021 to Feb 1, 2024"). */
function dayRange(a, b, short, year) {
  if (sameDay(a, b)) return dayOf(a, short, year);
  const f = short ? F.mdShort : F.md;
  const oneYear = a.getFullYear() === b.getFullYear();
  if (f && f.formatToParts && oneYear && a.getMonth() === b.getMonth()) {
    const pa = f.formatToParts(a), pb = f.formatToParts(b);
    const mi = pa.findIndex((p) => p.type === 'month'), di = pa.findIndex((p) => p.type === 'day');
    const db = pb.find((p) => p.type === 'day');
    const y = year ? a.getFullYear() : '';
    if (mi >= 0 && di >= 0 && db) return mi < di ? `${pa[mi].value} ${pa[di].value} to ${db.value}${y ? `, ${y}` : ''}` : `${pa[di].value} to ${db.value} ${pa[mi].value}${y ? ` ${y}` : ''}`;
  }
  if (year && oneYear) return `${dayOf(a, short)} to ${dayOf(b, short, true)}`;
  return `${dayOf(a, short, year)} to ${dayOf(b, short, year)}`;
}
/* "Mar 2019 to Jul 2021": the shortest form of a range that crosses years */
const monthRange = (a, b) => (a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() ? monthOf(a) : `${monthOf(a)} to ${monthOf(b)}`);
/* A day and, unless it is midnight, its time: "Jun 12, 8:00 AM", with the year when the run crosses one. */
function momentOf(t, year) {
  if (!year) return kit.formatTime(t, { withDate: true });
  const day = dayOf(t, true, true);
  if (t.getHours() === 0 && t.getMinutes() === 0) return day;
  return `${day}, ${F.time ? F.time.format(t) : kit.formatTime(t)}`;
}
/* A stay's length: its nights, or under a night its hours, or for a stay given as one bare date, a day. */
function nightsText(nights, ms, dated) {
  if (nights === 1) return '1 night';
  if (nights > 1) return `${nights} nights`;
  if (dated) return 'A day';
  const min = Math.round(ms / 60000);
  if (min <= 1) return '1 minute';
  if (min < 60) return `${min} minutes`;
  const h = Math.round(min / 60);
  return h === 1 ? '1 hour' : `${h} hours`;
}
function elapsed(ms) {
  const total = Math.round(ms / 60000);
  if (total < 60) return `${Math.max(0, total)} min`;
  const h = Math.floor(total / 60), m = total % 60;
  return m ? `${h} h ${m} m` : `${h} h`;
}
const capital = (s) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);

/* The midnights inside [a, b], on the reader's clock: one for each night of a stay. A stay that ends on a bare date
   (`through`) left during that day, so the midnight that starts it counts too. The list is for the dots, so it stops
   at 400, more than any capsule has room to draw; nightsIn counts them all. */
function midnights(a, b, through) {
  const out = [];
  for (let d = new Date(a.getFullYear(), a.getMonth(), a.getDate() + 1); (d < b || (through && +d === +b)) && out.length < 400; d = new Date(d.getFullYear(), d.getMonth(), d.getDate() + 1)) out.push(d);
  return out;
}
const startOfDay = (t) => new Date(t.getFullYear(), t.getMonth(), t.getDate());
function nightsIn(a, b, through) {
  const n = Math.round((startOfDay(b) - startOfDay(a)) / DAY_MS) - (!through && +b === +startOfDay(b) ? 1 : 0);
  return Math.max(0, n);
}
const BARE_DATE = /^\d{4}-\d{2}-\d{2}$/;
const bare = (v) => typeof v === 'string' && BARE_DATE.test(v.trim());

/* ---------- the model ---------- */
function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const find = (names) => names.find((n) => columns.includes(n)) || null;
  const placeK = find(['place', 'label', 'name', 'place_name', 'title']);
  const t0K = find(['started_at', 'start', 'arrived_at']), t1K = find(['ended_at', 'end', 'left_at']);
  const modeK = find(['mode', 'travel']);
  let stops = [];
  if (placeK && (t0K || t1K)) {
    stops = rows.slice(0, MAX_ROWS).map((r) => {
      const name = r[placeK];
      const mode = modeK && typeof r[modeK] === 'string' && r[modeK].trim() ? capital(r[modeK].trim().slice(0, 24)) : null;
      return { row: r, place: typeof name === 'string' && name.trim() ? name.trim() : UNNAMED, a: t0K ? kit.parseTime(r[t0K]) : null, b: t1K ? kit.parseTime(r[t1K]) : null, mode,
        aBare: !!t0K && bare(r[t0K]), bBare: !!t1K && bare(r[t1K]) };
    }).filter((s) => s.a || s.b);
  }
  const n = stops.length;
  stops.forEach((s, i) => {
    /* an end: the first row with no start, or the last with no end */
    s.end = n > 1 && ((i === 0 && !s.a) || (i === n - 1 && !s.b)) ? (i === 0 ? 'first' : 'last') : null;
    if (!s.end) {
      if (!s.a) { s.a = s.b; s.aBare = s.bBare; }
      if (!s.b || s.b < s.a) { s.b = s.a; s.bBare = s.aBare; }
      s.ms = s.b - s.a;
      s.days = s.ms / DAY_MS;
      s.nightsAt = midnights(s.a, s.b, s.bBare);
      s.nights = nightsIn(s.a, s.b, s.bBare);
    }
    s.at = s.end === 'first' ? s.b : s.a;           /* the one time an end has */
    s.atBare = s.end === 'first' ? s.bBare : s.aBare;
  });
  const legs = [];
  for (let i = 1; i < n; i++) {
    const from = stops[i - 1], to = stops[i];
    const t0 = from.b || from.a, t1 = to.a || to.b;
    /* a leg between bare dates has no clock time, so it has no length to give */
    const dated = (from.b ? from.bBare : from.aBare) || (to.a ? to.aBare : to.bBare);
    legs.push({ i, from, to, t0, t1, ms: Math.max(0, t1 - t0), mode: to.mode, dated });
  }
  /* a run that crosses a new year carries the year in every date it writes */
  const yrs = stops.flatMap((s) => [s.a, s.b, s.at]).filter(Boolean).map((t) => t.getFullYear());
  const years = yrs.length > 0 && Math.min(...yrs) !== Math.max(...yrs);
  return { stops, legs, years };
}

function ariaLabel(M) {
  const S = M.stops;
  if (!S.length) return 'Strip map of stays in order, with no stays to draw.';
  const piece = (s, k) => {
    const leg = k > 0 ? M.legs[k - 1] : null;
    const by = leg && leg.mode ? ` (${leg.mode.toLowerCase()})` : '';
    if (s.end === 'first') return `from ${s.place}, leaving ${dayOf(s.at, false, M.years)}`;
    if (s.end === 'last') return `to ${s.place}${by}, arriving ${dayOf(s.at, false, M.years)}`;
    return `${k ? 'to ' : ''}${s.place}${by}, ${dayRange(s.a, s.b, false, M.years)}, ${nightsText(s.nights, s.ms, s.bBare).replace(/^A day$/, 'a day')}`;
  };
  const first = S[0], last = S[S.length - 1];
  const t0 = first.end ? first.at : first.a, t1 = last.end ? last.at : last.b;
  return `Strip map of stays in order, ${dayRange(t0, t1, false, M.years)}: ${S.map(piece).join('; ')}.`;
}

/* ---------- the component ---------- */
export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-tripline');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: HEIGHT, readout: true, foot: opts.foot !== false, hint: HINT });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);
  const measure = document.createElement('canvas').getContext('2d');

  let out = output || {};
  let M = model(out);
  let width = 0;
  let G = null;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;    /* an item: 2k is stop k, 2k - 1 the leg into stop k */
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let clock = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);
  const itemCount = () => (M.stops.length ? M.stops.length * 2 - 1 : 0);

  /* ---------- layout: each stop's place along the line, then the words ---------- */
  function layout() {
    G = null;
    const S = M.stops;
    if (!width || !S.length) return;
    const b = box(width);
    const x0 = b.left + EDGE, A = Math.max(40, b.right - EDGE - x0);
    const legs = S.length - 1;
    const stays = S.filter((s) => !s.end);
    const endsW = (S.length - stays.length) * 2 * END_R;
    /* the stays take about 60% of the line and the legs share the rest, each at least LEG_MIN */
    let share = stays.length ? (legs ? 0.6 : 1) * (A - endsW) : 0;
    if (legs) share = Math.min(share, A - endsW - legs * LEG_MIN);
    share = Math.max(share, stays.length * CAP_H);
    const d = stays.map((s) => Math.min(s.days, MAX_DAYS));
    let u = share / Math.max(1e-6, d.reduce((a, x) => a + x, 0));
    for (let it = 0; it < 4; it++) {
      let fixed = 0, rest = 0;
      for (const x of d) { if (u * x < CAP_H) fixed += CAP_H; else rest += x; }
      if (rest > 0) u = Math.max(0, share - fixed) / rest;
    }
    u = Math.min(u, U_MAX);
    for (const s of S) s.w = s.end ? 2 * END_R : Math.max(CAP_H, u * Math.min(s.days, MAX_DAYS));
    const sumW = S.reduce((a, s) => a + s.w, 0);
    const leg = legs ? kit.clamp((A - sumW) / legs, 4, LEG_MAX) : 0;
    const used = sumW + legs * leg;
    let x = x0 + Math.max(0, (A - used) / 2);
    for (const s of S) {
      s.x0 = x; s.x1 = x + s.w; s.cx = x + s.w / 2; x = s.x1 + leg;
      s.nightX = [];
      if (s.end || !s.nightsAt.length) continue;
      const xs = s.nightsAt.map((m) => kit.clamp(s.x0 + (s.ms ? (m - s.a) / s.ms : 0.5) * s.w, s.x0 + CAP_H / 2, s.x1 - CAP_H / 2));
      if (xs.every((v, k) => !k || v - xs[k - 1] >= 3.5)) s.nightX = xs;
    }
    for (const lg of M.legs) lg.cx = (lg.from.x1 + lg.to.x0) / 2;
    G = { b, S, leg, names: [], dates: [], nights: [], modes: [] };
    placeLabels();
    if (mode === 'live') markBorn();
  }

  function textW(text, font) { measure.font = font; return measure.measureText(text).width; }
  /* Labels centered on their marks and kept inside the plot. Where two touch, the one that matters less goes: the
     ends first, then the longer stays. */
  function settle(items, gap) {
    const b = G.b;
    for (const it of items) {
      it.x0 = kit.clamp(it.cx - it.w / 2, b.left + 1, b.right - 1 - it.w);
      it.x1 = it.x0 + it.w;
    }
    const clash = items.some((it, k) => k && it.x0 < items[k - 1].x1 + gap);
    return clash;
  }
  function greedy(items, gap) {
    const order = items.slice().sort((p, q) => q.rank - p.rank);
    const kept = [];
    for (const it of order) if (!kept.some((q) => it.x0 < q.x1 + gap && it.x1 > q.x0 - gap)) kept.push(it);
    return kept.sort((p, q) => p.x0 - q.x0);
  }
  function placeLabels() {
    const S = G.S, T = tokens;
    const rank = (s) => (s.end ? 1e6 : s.days);
    /* names in the serif: 15px, then 13px, then only those with room */
    let names = null;
    for (const px of NAME_PX) {
      const font = `400 ${px}px ${T.serif}`;
      const items = S.map((s, k) => ({ k, text: s.place, font, cx: s.cx, w: textW(s.place, font), rank: rank(s) }));
      if (!settle(items, 10)) { names = items; break; }
      names = items;
    }
    G.names = settle(names, 10) ? greedy(names, 10) : names;
    /* dates, then the nights under them, in the chart's mono */
    const mono = `9.5px ${T.mono}`;
    /* long months, then short ones, then the month only where it changes from the stop before. A run that crosses
       a year writes the year too, and at the last level only the month and year. */
    const dateText = (s, k, level) => {
      if (M.years) {
        if (level === 2) return s.end ? monthOf(s.at) : monthRange(s.a, s.b);
        return s.end ? dayOf(s.at, level > 0, true) : dayRange(s.a, s.b, level > 0, true);
      }
      if (s.end) return dayOf(s.at, level > 0);
      const prev = k > 0 ? S[k - 1] : null, before = prev ? prev.b || prev.at : null;
      if (level === 2 && before && before.getMonth() === s.a.getMonth() && before.getFullYear() === s.a.getFullYear()) {
        return sameDay(s.a, s.b) ? String(s.a.getDate()) : s.a.getMonth() === s.b.getMonth() ? `${s.a.getDate()} to ${s.b.getDate()}` : dayRange(s.a, s.b, true);
      }
      return dayRange(s.a, s.b, level > 0);
    };
    let dates = null;
    for (const level of [0, 1, 2]) {
      const items = S.map((s, k) => { const text = dateText(s, k, level); return { k, text, font: mono, cx: s.cx, w: textW(text, mono), rank: rank(s) }; });
      dates = items;
      if (!settle(items, 8)) break;
    }
    G.dates = settle(dates, 8) ? greedy(dates, 8) : dates;
    const keptDates = new Set(G.dates.map((it) => it.k));
    const nights = S.map((s, k) => ({ k, text: s.end ? END_WORD[s.end] : nightsText(s.nights, s.ms, s.bBare), font: mono, cx: s.cx, rank: rank(s) }))
      .filter((it) => keptDates.has(it.k));
    for (const it of nights) it.w = textW(it.text, mono);
    G.nights = settle(nights, 8) ? greedy(nights, 8) : nights;
    /* each leg's mode word, above the line, while it fits between the middles of the stops either side */
    G.modes = M.legs.filter((lg) => lg.mode).map((lg) => {
      const w = textW(lg.mode, mono);
      return { leg: lg.i, text: lg.mode, font: mono, cx: lg.cx, w, x0: lg.cx - w / 2, x1: lg.cx + w / 2, room: lg.to.cx - lg.from.cx - 6 };
    }).filter((it) => it.w <= it.room);
  }

  /* ---------- drawing ---------- */
  function grow(s) {
    if (mode === 'none' || reduced()) return 1;
    if (s.born === undefined) return 0;
    return kit.clamp(kit.stepSpring(clock - s.born, 0.42, 0.22), 0, 1.2);
  }
  function fade(born) {
    if (mode === 'none' || reduced()) return 1;
    if (born === undefined) return 0;
    return kit.seg(clock - born, 0.12, 0.42);
  }
  function capsule(ctx, x0, x1, y, h) {
    const r = h / 2, w = Math.max(0, x1 - x0);
    ctx.beginPath();
    if (w <= h) { ctx.arc((x0 + x1) / 2, y, r, 0, TAU); return; }
    ctx.moveTo(x0 + r, y - r);
    ctx.lineTo(x1 - r, y - r);
    ctx.arc(x1 - r, y, r, -Math.PI / 2, Math.PI / 2);
    ctx.lineTo(x0 + r, y + r);
    ctx.arc(x0 + r, y, r, Math.PI / 2, Math.PI * 1.5);
    ctx.closePath();
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    if (!G) return;
    const T = tokens, S = G.S;
    const live = mode === 'live' && !reduced();
    const head = live && reveal < Infinity ? reveal : Infinity;
    const col = T.rgb.s0;
    const hvStop = hover !== null && hover % 2 === 0 ? hover / 2 : null;
    const hvLeg = hover !== null && hover % 2 === 1 ? (hover + 1) / 2 : null;
    const dimA = kit.lerp(1, 0.4, dim.x);
    const lit = (k) => hover === null || hvStop === k;

    /* the line, drawn as far as the frame's sweep has come */
    const lx0 = S[0].cx, lx1 = S[S.length - 1].cx;
    const lxEnd = Math.min(lx1, head);
    if (lxEnd > lx0 || S.length === 1) {
      ctx.save();
      ctx.strokeStyle = kit.rgba(col);
      ctx.lineWidth = LINE_W;
      ctx.lineCap = 'round';
      ctx.beginPath(); ctx.moveTo(lx0, LINE_Y); ctx.lineTo(Math.max(lx0, lxEnd), LINE_Y); ctx.stroke();
      ctx.restore();
    }

    /* the stations and the ends */
    S.forEach((s, k) => {
      const g = grow(s);
      if (g <= 0) return;
      const a = lit(k) ? 1 : dimA;
      ctx.save();
      ctx.fillStyle = T.page;
      ctx.lineWidth = 1.5;
      ctx.strokeStyle = kit.rgba(hvStop === k ? col : T.rgb.ink, (hvStop === k ? col[3] : T.rgb.ink[3]) * a);
      if (s.end) {
        ctx.beginPath(); ctx.arc(s.cx, LINE_Y, END_R * g, 0, TAU); ctx.fill(); ctx.stroke();
      } else {
        const hw = (s.w / 2) * Math.min(1, g), hh = CAP_H * g;
        capsule(ctx, s.cx - hw, s.cx + hw, LINE_Y, hh); ctx.fill(); ctx.stroke();
        /* a dot at each midnight of the stay, where the capsule has room to keep them apart */
        if (g >= 0.9 && s.nightX.length) {
          ctx.fillStyle = kit.rgba(T.rgb.s1, T.rgb.s1[3] * a * kit.clamp((g - 0.9) * 10, 0, 1));
          ctx.beginPath();
          for (const x of s.nightX) { ctx.moveTo(x + NIGHT_R, LINE_Y); ctx.arc(x, LINE_Y, NIGHT_R, 0, TAU); }
          ctx.fill();
        }
      }
      ctx.restore();
    });

    /* the ring around what you rest on */
    if (hover !== null) {
      ctx.save();
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(col, col[3] * 0.9);
      const grow2 = 2.5 + 2.5 * ring.x;
      if (hvStop !== null) {
        const s = S[hvStop];
        if (s.end) { ctx.beginPath(); ctx.arc(s.cx, LINE_Y, END_R + grow2, 0, TAU); }
        else capsule(ctx, s.x0 - grow2, s.x1 + grow2, LINE_Y, CAP_H + grow2 * 2);
        ctx.stroke();
      } else if (hvLeg !== null) {
        const lg = M.legs[hvLeg - 1];
        ctx.fillStyle = kit.rgba(col);
        ctx.beginPath(); ctx.arc(lg.cx, LINE_Y, 3.2, 0, TAU); ctx.fill();
        ctx.beginPath(); ctx.arc(lg.cx, LINE_Y, 4.5 + 3 * ring.x, 0, TAU); ctx.stroke();
      }
      ctx.restore();
    }

    /* the words: names above, dates and nights below, modes over their legs */
    ctx.save();
    ctx.textAlign = 'left';
    ctx.textBaseline = 'alphabetic';
    const words = (list, y, color, keyed) => {
      for (const it of list) {
        const k = keyed(it);
        const s = it.leg !== undefined ? M.legs[it.leg - 1] : S[k];
        const a = fade(it.leg !== undefined ? s.born : S[k].born) * (it.leg !== undefined ? (hover === null || hvLeg === it.leg ? 1 : dimA) : (lit(k) ? 1 : dimA));
        if (a <= 0) continue;
        ctx.globalAlpha = a;
        ctx.font = it.font;
        ctx.fillStyle = typeof color === 'function' ? color(it) : color;
        ctx.fillText(it.text, it.x0, y);
      }
    };
    words(G.names, NAME_Y, T.ink, (it) => it.k);
    words(G.dates, DATE_Y, (it) => (hvStop === it.k ? T.ink : T.muted), (it) => it.k);
    words(G.nights, NIGHT_Y, T.subtle, (it) => it.k);
    words(G.modes, MODE_Y, (it) => (hvLeg === it.leg ? T.ink : T.muted), (it) => it.leg);
    ctx.restore();
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    const S = M.stops;
    if (!S.length) { kit.setReadout(parts.readout, []); return; }
    if (hover === null) {
      const first = S[0], last = S[S.length - 1];
      const t0 = first.end ? first.at : first.a, t1 = last.end ? last.at : last.b;
      const through = last.end ? last.atBare : last.bBare;
      kit.setReadout(parts.readout, [{ when: dayRange(t0, t1, false, M.years) }, { label: first.end || last.end ? 'In all' : '', value: nightsText(nightsIn(t0, t1, through), t1 - t0, through) }]);
      return;
    }
    if (hover % 2 === 0) {
      const s = S[hover / 2];
      if (s.end) kit.setReadout(parts.readout, [{ when: s.place }, { label: END_WORD[s.end], value: momentOf(s.at, M.years) }]);
      else kit.setReadout(parts.readout, [{ when: s.place }, { label: dayRange(s.a, s.b, false, M.years), value: nightsText(s.nights, s.ms, s.bBare) }]);
      return;
    }
    const lg = M.legs[(hover + 1) / 2 - 1];
    const day = M.years ? dayOf(lg.t0, true, true) : kit.formatTime(lg.t0, { withDate: true, withTime: false });
    kit.setReadout(parts.readout, [{ when: `${lg.from.place} to ${lg.to.place}` }, lg.dated ? { label: lg.mode || '', value: day } : { label: lg.mode ? `${lg.mode}, ${day}` : day, value: elapsed(lg.ms) }]);
  }

  function setHover(k, from) {
    if (k === hover) { hoverFrom = from; return; }
    const was = hover;
    hover = k; hoverFrom = from;
    if (k !== null) { ring.x = reduced() ? 1 : was === null ? 0 : 0.4; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while the line draws, stations open, a ring grows or the others dim ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    const target = hover !== null ? 1 : 0;
    if (reduced()) { ring.x = 1; ring.v = 0; dim.x = target; dim.v = 0; }
    else {
      kit.springTo(ring, 1, dt, 0.28, 0.15);
      kit.springTo(dim, target, dt, 0.2, 0);
      moving = ring.x !== 1 || dim.x !== target;
    }
    if (mode === 'live' && !reduced()) {
      if (reveal < Infinity) moving = true;
      for (const s of M.stops) if (s.born === undefined || clock - s.born < 1.1) { moving = true; break; }
      for (const lg of M.legs) if (lg.born === undefined || clock - lg.born < 0.5) { moving = true; break; }
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    if (!G) return;
    for (const s of M.stops) if (s.born === undefined && (reveal === Infinity || s.x0 <= reveal)) s.born = clock;
    for (const lg of M.legs) if (lg.born === undefined && (reveal === Infinity || lg.cx <= reveal)) lg.born = clock;
  }
  function rowsWait(on) {
    for (const node of [parts.readout, parts.foot]) {
      if (!node) continue;
      node.classList.toggle('vk-fade', !on && !reduced());
      node.classList.toggle('vk-wait', on);
    }
  }
  function setReveal(x) {
    if (mode === 'none' && x === Infinity) return;
    if (mode === 'none') mode = 'live';
    reveal = x;
    markBorn();
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  /* the stop under the pointer, or the leg between two stops; a stop reaches 4px past its ends */
  function nearest(px) {
    const S = G.S;
    for (let k = 0; k < S.length; k++) {
      if (k > 0 && px < S[k].x0 - 4) return 2 * k - 1;
      if (px <= S[k].x1 + 4) return 2 * k;
    }
    return itemCount() - 1;
  }
  const pick = (e) => nearest(e.clientX - canvas.getBoundingClientRect().left);
  const ready = () => !!G && (mode === 'none' || reveal === Infinity);
  const onMove = (e) => {
    if (!ready()) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!ready()) return;
    const k = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === k) { setHover(null, null); return; }
    setHover(k, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(parts.plot, {
    count: () => (ready() ? itemCount() : 0),
    get: () => hover,
    set: (k, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(k, 'key');
    },
    page: 4,
  });

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
    hover = null; hoverFrom = null;
    layout();
    kit.setLabel(parts.plot, ariaLabel(M));
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') { rowsWait(reveal < Infinity); loop.wake(); }
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    layout();
    draw();
    if (mode === 'live') loop.wake();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; layout(); draw(); }
  });
  const stopDpr = kit.watchDpr(() => draw());
  const stopMotion = kit.watchReducedMotion(() => { rowsWait(mode === 'live' && reveal < Infinity); loop.wake(); });
  /* the names are measured in the serif, so they are placed again once the page's fonts arrive */
  const fonts = typeof document !== 'undefined' && document.fonts && document.fonts.addEventListener ? document.fonts : null;
  const onFonts = () => { layout(); draw(); };
  if (fonts) fonts.addEventListener('loadingdone', onFonts);
  apply();
  if (mode === 'live') loop.wake();

  return {
    plot: parts.plot,
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      out = next || {};
      apply();
    },
    setReveal,
    destroy() {
      loop.stop(); stopKeys(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      if (fonts) fonts.removeEventListener('loadingdone', onFonts);
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointerleave', onLeave);
      root.remove();
    },
  };
}
