// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/loupe.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* loupe.js: an actogram. A long run of days in one stippled strip, each day a column of dots from the start to the
   end of a daily interval, read through a loupe. It shows when the interval starts and ends across many days, too
   many to read as bars: when sleep falls across a year, or when any daily window opens and closes.

   Each day is a short column of fine dots on a clock, from the interval's start at the top to its end at the foot,
   so a year sits in one stippled ribbon and its seasons and weekends show in the edges (lab 6's loupe tile). Press
   and hold, and a glass loupe rises where you press: the days under it spread into clean columns and resolve into
   minutes, the day you are reading turns to ink with its two times beside it, and that day's sentence sits beneath
   the glass. Drag to read the next day. Let go and the refraction drains away until the glass is gone.
   The sentence names the times with the start and end columns' own names, so the demo's asleep_at and woke_at read
   "Asleep at 11:02 PM, woke at 7:10 AM" and started_at and ended_at read "Started at ..., ended at ...".
   - The glass is a floating control, so glass is allowed. It stays inside the plot and never passes over the
     reply's words. On a touch screen it rises 56px above the finger, after a short hold so a scroll does not
     raise it.
   - Where WebGL2 is available the glass refracts at its rim. Otherwise, or with options.webgl === false, a canvas
     2D glass magnifies the same way without refraction. The WebGL context is made on the first press, so a strip
     nobody presses holds none.
   - One accent: the days are the primary, the day under the glass is ink. Nothing is scored or flagged.
   - A missing day is an empty slot. A day whose interval breaks can arrive as two rows with one date, and the gap
     between them stays empty under the glass.

   The contract it proposes: a new chart mark, mark "actogram".
     columns   day, a date, then started_at and ended_at, timestamps. night, asleep_at and woke_at are read as
               aliases of them; without those names the columns are read in that order. Nothing else.
     rows      one per day, or one per stretch when a day's interval breaks. Up to 400 rows.
   show.rs's validation, the byte budget behind 400 and the tool description are in charts/entries/loupe.json.

   Block: the 26px readout row (a legend at rest), the 200px plot, and the 26px foot row, which holds "From your
   records" and "See the rows" at rest and the day's sentence while the glass is up. 252px, like series, so the
   frame can hold it. The sentence is the readout: aria-live, polite.
   Keyboard: focus the strip, then the left and right arrow keys move the loupe day by day (Home, End, Page Up
   and Page Down too), the up and down arrow keys move it along the clock, and Escape or leaving puts it down.

   mount(el, output, options) -> { update(output, patch), destroy(), plot, setReveal(x), glass }
     options   the shared ones (onOpenRows, reduced, foot, reveal), and webgl: false for the canvas 2D glass.
     setReveal the frame brings the days in behind its dot, fading over the 48px behind it.
     glass     'gl' or '2d' once the loupe has been pressed, else null. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'loupe', kinds: ['chart'], marks: ['actogram'] });

const HEIGHT = kit.PLOT.height;
const PAD = Object.freeze({ top: 10, right: 8, bottom: 22, left: 40 });
const END_R = 1.75;
const REST_STEP = 2.6, REST_R = 0.7;      /* a night at rest: a dot every 2.6px down its column */
const FINE_R = 0.78, FOCUS_R = 0.95;      /* under the glass: a dot every few minutes */
const LENS_HH = 52, LENS_PAD = 26;        /* the glass is 104px tall; its content is drawn 26px past the rim */
const MY = 2.3;                           /* magnification along the clock, the lab's default */
const RISE = 0.22, DRAIN = 0.26;          /* the refraction rises over 220ms and drains over 260ms */
const TOUCH_LIFT = 56, TOUCH_HOLD = 0.12;
const NIGHT_ALPHA = 0.62;
const TAU = Math.PI * 2;

export function blockHeight() {
  return kit.ROWS.readout + HEIGHT + kit.ROWS.foot;
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- the nights, from the strip's rows ---------- */

const localDay = (t) => Math.round(Date.UTC(t.getFullYear(), t.getMonth(), t.getDate()) / 864e5);
/* Minutes after the night's own midnight on the wall clock, so 11:48 pm the evening before is -12. */
const clockMinutes = (t, day) => (localDay(t) - day) * 1440 + t.getHours() * 60 + t.getMinutes() + t.getSeconds() / 60;
const hash = (n) => { const s = Math.sin(n * 91.7 + 12.9) * 43758.5453; return s - Math.floor(s); };

function clockLabel(m) {
  const mm = ((Math.round(m) % 1440) + 1440) % 1440;
  return new Date(2000, 0, 1, Math.floor(mm / 60), mm % 60).toLocaleTimeString(undefined, { hour: 'numeric' });
}

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  /* day, started_at and ended_at by name, case aside, with night, asleep_at and woke_at as aliases; else by place */
  const pick = (names, at) => names.map((n) => columns.find((c) => typeof c === 'string' && c.trim().toLowerCase() === n)).find(Boolean) || columns[at];
  const key = pick(['day', 'night'], 0), aKey = pick(['started_at', 'asleep_at'], 1), wKey = pick(['ended_at', 'woke_at'], 2);
  const byDay = new Map();
  for (const r of rows) {
    const d = kit.parseTime(r[key]), a = kit.parseTime(r[aKey]), w = kit.parseTime(r[wKey]);
    if (!d || !a || !w || w <= a) continue;
    const day = localDay(d);
    let n = byDay.get(day);
    if (!n) { n = { day, date: new Date(d.getFullYear(), d.getMonth(), d.getDate()), spans: [] }; byDay.set(day, n); }
    n.spans.push({ a: clockMinutes(a, day), w: clockMinutes(w, day), at: a, wt: w });
  }
  const nights = [...byDay.values()].sort((x, y) => x.day - y.day);
  const M = { columns, key, aKey, wKey, nouns: nouns(key), nights, nSlots: 0, lo: 0, hi: 1, guides: [], slotNight: [], nearest: [] };
  if (!nights.length) return M;
  nights.forEach((n, i) => {
    n.spans.sort((p, q) => p.a - q.a);
    n.i = i;
    n.a = n.spans[0].a;
    n.w = Math.max(...n.spans.map((s) => s.w));
    n.ph = hash(n.day * 1.37);
    n.jx = hash(n.day * 7.91);
  });
  const first = nights[0].day;
  M.nSlots = nights[nights.length - 1].day - first + 1;
  M.firstDate = nights[0].date;
  M.slotNight = new Array(M.nSlots).fill(null);
  for (const n of nights) { n.slot = n.day - first; M.slotNight[n.slot] = n; }
  /* the night nearest each slot, for a pointer between two nights */
  let prev = null;
  const next = new Array(M.nSlots).fill(null);
  for (let s = M.nSlots - 1, nx = null; s >= 0; s--) { if (M.slotNight[s]) nx = M.slotNight[s]; next[s] = nx; }
  for (let s = 0; s < M.nSlots; s++) {
    if (M.slotNight[s]) prev = M.slotNight[s];
    const a = prev, b = next[s];
    M.nearest[s] = !a ? b : !b ? a : (s - a.slot <= b.slot - s ? a : b);
  }
  let lo = Math.min(...nights.map((n) => n.a)), hi = Math.max(...nights.map((n) => n.w));
  lo = Math.floor(lo / 60) * 60;
  hi = Math.ceil(hi / 60) * 60;
  if (hi - lo < 240) { lo = Math.floor(((lo + hi) / 2 - 120) / 60) * 60; hi = lo + 240; }
  M.lo = lo; M.hi = hi;
  const gStep = hi - lo <= 360 ? 60 : hi - lo <= 960 ? 120 : 180;
  for (let m = Math.ceil(lo / gStep) * gStep; m <= hi; m += gStep) M.guides.push(m);
  return M;
}

/* What one day is called, from the day column's name: "Night" gives night and nights. Another name, or one of more
   than one word, gives day and days. */
function nouns(label) {
  const w = String(label || '').trim().toLowerCase();
  if (w === 'night') return ['night', 'nights'];
  return ['day', 'days'];
}

/* ---------- the day's sentence ---------- */

/* The words before each time: the start and end columns' own names, so asleep_at reads "Asleep at" and ended_at
   "ended at". */
function timeWords(M) {
  const lab = (c, fallback) => (typeof c === 'string' && c.trim() ? kit.columnLabel(c.trim()) : fallback);
  const a = lab(M.aKey, 'Started at'), w = lab(M.wKey, 'Ended at');
  return [a.charAt(0).toUpperCase() + a.slice(1), w.charAt(0).toLowerCase() + w.slice(1)];
}

/* The longest form that fits the column is used, so a break in the interval keeps its times wherever there is room. */
function sentences(n, words) {
  const [aw, ww] = words;
  const t = (d) => kit.formatTime(d);
  /* "3:12 to 3:50 AM" when both times end the same way */
  const range = (a, b) => {
    const ta = t(a), tb = t(b), sa = ta.match(/\s*\D+$/), sb = tb.match(/\s*\D+$/);
    return sa && sb && sa[0] === sb[0] ? `${ta.slice(0, -sa[0].length)} to ${tb}` : `${ta} to ${tb}`;
  };
  const long = n.date.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' });
  const mid = n.date.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
  const short = n.date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  const from = t(n.spans[0].at);
  const to = t(n.spans.reduce((a, s) => (s.wt > a ? s.wt : a), n.spans[0].wt));
  const gaps = [];
  for (let k = 1; k < n.spans.length; k++) if (n.spans[k].at > n.spans[k - 1].wt) gaps.push([n.spans[k - 1].wt, n.spans[k].at]);
  if (!gaps.length) {
    return [
      `${long}. ${aw} ${from}, ${ww} ${to}.`,
      `${mid}. ${aw} ${from}, ${ww} ${to}.`,
      `${mid}. ${from} to ${to}.`,
      `${short}. ${from} to ${to}.`,
    ];
  }
  const gap = gaps.length === 1 ? `with a gap from ${range(gaps[0][0], gaps[0][1])}` : 'with a few gaps';
  return [
    `${long}. ${aw} ${from}, ${ww} ${to}, ${gap}.`,
    `${mid}. ${aw} ${from}, ${ww} ${to}, ${gap}.`,
    `${mid}. ${from} to ${to}, ${gap}.`,
    `${short}. ${from} to ${to}, with a gap.`,
  ];
}

/* ---------- the glass in WebGL2: the lab's rim refraction, drawn over the strip with alpha ---------- */

const VS = `#version 300 es
void main(){vec2 p=vec2(float((gl_VertexID<<1)&2),float(gl_VertexID&2));gl_Position=vec4(p*2.-1.,0.,1.);}`;
const FS = `#version 300 es
precision highp float;
uniform sampler2D uLens;
uniform vec2 uRes, uC, uHalf, uLO, uLS;
uniform float uS, uRad, uStr, uAng, uStretch, uHiA, uShA;
uniform vec3 uInk, uHi, uShC;
out vec4 o;
float sdRR(vec2 p, vec2 b, float r){ vec2 q = abs(p) - b + r; return length(max(q, 0.)) + min(max(q.x, q.y), 0.) - r; }
vec2 rot(vec2 v, float a){ float c = cos(a), s = sin(a); return vec2(c*v.x - s*v.y, s*v.x + c*v.y); }
float sdf(vec2 p){ vec2 q = rot(p - uC, -uAng); q.x /= uStretch; q.y *= uStretch; q = rot(q, uAng); return sdRR(q, uHalf, uRad); }
vec3 lensAt(vec2 p){ vec2 uv = (p - uLO) / uLS; return texture(uLens, vec2(uv.x, 1. - uv.y)).rgb; }
void main(){
  vec2 p = vec2(gl_FragCoord.x, uRes.y - gl_FragCoord.y) / uS;
  float d = sdf(p);
  float aa = .8 / uS;
  float sh = uShA * uStr * (1. - smoothstep(-8., 24., sdf(p - vec2(0., 7.))));
  if (d > aa) { o = vec4(uShC * sh, sh); return; }
  float e = -d;
  vec2 n = normalize(vec2(sdf(p + vec2(.5, 0.)) - sdf(p - vec2(.5, 0.)), sdf(p + vec2(0., .5)) - sdf(p - vec2(0., .5))) + 1e-5);
  float b = 14.;
  vec2 sp = p;
  if (e < b) {
    float t = clamp(1. - e / b, 0., .999);
    float hp = t / (b * sqrt(1. - t * t));
    sp = p + n * min(hp * b, 3.) * 22. * (1. - 1. / 1.5) / 3. * uStr;
  }
  vec3 c = lensAt(sp);
  c = mix(c, uInk, .12 * uStr * (1. - smoothstep(0., .9, abs(e - .6))));
  float hl = (1. - smoothstep(.2, 1.6, abs(e - 1.4))) * smoothstep(.2, .9, dot(-n, normalize(vec2(1., 1.))));
  c = mix(c, uHi, uHiA * hl * uStr);
  float a = smoothstep(aa, -aa, d);
  o = vec4(c * a + uShC * sh * (1. - a), a + sh * (1. - a));
}`;

function makeGL(canvas) {
  let gl = null;
  try { gl = canvas.getContext('webgl2', { alpha: true, premultipliedAlpha: true, antialias: false, depth: false, stencil: false }); } catch (err) { gl = null; }
  if (!gl) return null;
  const shader = (type, src) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, src); gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) { console.error(`loupe: ${gl.getShaderInfoLog(s)}`); return null; }
    return s;
  };
  const vs = shader(gl.VERTEX_SHADER, VS), fs = shader(gl.FRAGMENT_SHADER, FS);
  if (!vs || !fs) return null;
  const prog = gl.createProgram();
  gl.attachShader(prog, vs); gl.attachShader(prog, fs); gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) { console.error(`loupe: ${gl.getProgramInfoLog(prog)}`); return null; }
  const u = {};
  for (const n of ['uLens', 'uRes', 'uC', 'uHalf', 'uLO', 'uLS', 'uS', 'uRad', 'uStr', 'uAng', 'uStretch', 'uHiA', 'uShA', 'uInk', 'uHi', 'uShC']) u[n] = gl.getUniformLocation(prog, n);
  const tex = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, tex);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  const vao = gl.createVertexArray();
  return { gl, prog, u, tex, vao };
}

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  let out = output || {};
  let M = model(out);
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());

  const root = kit.h('div', 'vk vk-loupe');
  el.appendChild(root);
  const parts = kit.block(root, {
    plotHeight: HEIGHT, readout: true, foot: true,
    hint: 'Press and hold to look closer. With the keyboard, the left and right arrow keys move the loupe from day to day, and the up and down arrow keys move it along the clock.',
  });
  parts.plot.style.cursor = 'crosshair';
  parts.plot.style.userSelect = 'none';
  parts.plot.style.webkitUserSelect = 'none';
  const base = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(base);
  /* The night's sentence, beneath the glass in the foot row. It is the readout: aria-live. */
  const said = kit.h('div', 'vk-loupe-said', { 'aria-live': 'polite', 'aria-atomic': 'true' });
  said.style.cssText = `position:absolute;left:0;top:${kit.ROWS.readout + HEIGHT + 8}px;height:18px;line-height:18px;white-space:nowrap;pointer-events:none;opacity:0;font-style:normal;font-weight:400;font-size:15px`;
  parts.root.appendChild(said);

  let tokens = kit.readTokens(root);
  let width = 0;
  let glass = null;                 /* { kind: 'gl' | '2d', cv, ... } once the loupe has been pressed */
  let glFailed = opts.webgl === false;
  let lensCv = null;
  let baseDirty = true;
  /* the loupe */
  let active = false, kbd = false, pending = false, pendingAt = 0, str = 0, wasShown = false;
  let ptr = null;
  const target = { fx: 0, fy: 0, cx: 0, cy: 0 };
  const cxS = { x: 0, v: 0 }, cyS = { x: 0, v: 0 }, stretch = { x: 1, v: 0 };
  let ang = 0, keyY = null, focus = null, saidFor = null, saidW = 0, clock = 0;
  /* the frame's reveal: the nights show left of revealDraw, which follows the frame's sweep and, once the frame lets
     go, runs on past the right edge */
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let revealDraw = reveal;
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const B = () => box(width);
  const slotW = () => { const b = B(); return (b.right - b.left) / Math.max(1, M.nSlots); };
  const X = (s) => B().left + (s + 0.5) * slotW();
  const Y = (m) => { const b = B(); return b.top + ((m - M.lo) / (M.hi - M.lo || 1)) * (b.bottom - b.top); };
  const pxPerMin = () => { const b = B(); return (b.bottom - b.top) / (M.hi - M.lo || 1); };
  const lensHW = () => kit.clamp(width * 0.16, 60, 92);
  const MX = () => kit.clamp(7 / Math.max(0.1, slotW()), 2.2, 9);
  function fineStep() {
    const per = pxPerMin() * MY;
    return [5, 10, 15, 30, 60].find((m) => m * per >= 2.4) || 60;
  }

  /* ---------- painting: the same drawing at rest (map is the identity) and under the glass ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (path, x, y, r) => { path.moveTo(x + r, y); path.arc(x, y, r, 0, TAU); };

  /* A night at rest: dots down its column with their own phase, and a little sideways jitter where the slots are
     narrow, so the year reads as one stippled ribbon. */
  function restDots(n, emit) {
    const sw = slotW(), x0 = X(n.slot), jit = sw < 3 ? Math.min(1.05, 0.15 + sw * 0.6) : 0;
    let k = 0;
    for (const s of n.spans) {
      const y1 = Y(s.w);
      for (let y = Y(s.a) + (jit ? n.ph * REST_STEP : 0); y <= y1 + 0.01; k++) {
        const hh = jit ? hash(n.day * 13.7 + k * 3.917) : 0.5;
        emit(x0 + jit * Math.sin(k * 2.39996 + n.jx * 40), y);
        y += REST_STEP * (jit ? 0.55 + 0.9 * hh : 1);
      }
    }
  }
  /* A day under the glass: a dot every few minutes, from the minute its interval started. */
  function fineDots(n, emit, every) {
    for (const s of n.spans) for (let m = s.a; m <= s.w + 0.01; m += every) emit(X(n.slot), Y(m));
  }

  /* map: { x(px), y(px), mx, my, x0, x1 } where x0..x1 is the span of the strip to draw (in rest px). */
  function paintFrame(ctx, map) {
    const T = tokens, b = B();
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    /* the clock's guides, the floor and the axis */
    for (const m of M.guides) {
      const y = Math.round(map.y(Y(m))) + 0.5;
      if (Math.abs(y - map.y(floor)) < 1.5) continue;
      dotted(ctx, map.x(L), y, map.x(R), y, 5, 1.1, T.borderStrong);
    }
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(map.x(L), Math.round(map.y(floor)), map.x(R) - map.x(L), 1);
    dotted(ctx, map.x(L), map.y(floor), map.x(L), map.y(top), 5.5, 1.3, T.muted);
    const ends = new Path2D();
    disc(ends, map.x(L), map.y(top), END_R); disc(ends, map.x(L), map.y(floor), END_R);
    ctx.fillStyle = T.ink; ctx.fill(ends);
    /* labels: the clock in the margin and the months along the foot, in the chart's 9.5px mono */
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    for (const m of M.guides) ctx.fillText(clockLabel(m), map.x(L) - 6, map.y(Y(m)));
    ctx.textAlign = 'left'; ctx.textBaseline = 'alphabetic';
    if (M.firstDate) {
      /* every month, or every second or third where a month is narrower than its label */
      const f = M.firstDate, sw0 = slotW() * map.mx;
      const every = [1, 2, 3, 6].find((k) => 28 * sw0 * k >= ctx.measureText('May').width + 8) || 12;
      let edge = -Infinity;
      for (let s = 0; s < M.nSlots; s++) {
        const d = new Date(f.getFullYear(), f.getMonth(), f.getDate() + s);
        if (d.getDate() !== 1 && s !== 0) continue;
        if (s === 0 && M.nSlots > 20 && d.getDate() > 20) continue;
        if (((d.getFullYear() - f.getFullYear()) * 12 + d.getMonth() - f.getMonth()) % every) continue;
        const text = d.toLocaleDateString(undefined, { month: 'short' });
        const x = map.x(L + s * slotW());
        const w = ctx.measureText(text).width;
        if (x < edge + 6 || x + w > map.x(R) + 4) continue;
        ctx.fillText(text, x, map.y(HEIGHT - 6));
        edge = x + w;
      }
    }
  }

  /* The nights' dots as [x, y, r, weight] in map px: at rest each night's stippled column, and under the glass
     its minute dots, crossfading as the glass rises (e is how far it has risen, 0 at rest). The night under the
     glass gives only its resting dots here; paintFocus draws its minutes in ink. */
  function nightDots(map, e, focusN) {
    const L = B().left, sw = slotW();
    const rest = e < 0.999, fine = e > 0.001;
    const s0 = Math.max(0, Math.floor((map.x0 - L) / sw) - 1), s1 = Math.min(M.nSlots - 1, Math.ceil((map.x1 - L) / sw) + 1);
    const restR = REST_R * map.my, every = fine ? fineStep() : 60;
    const dots = [];
    for (let s = s0; s <= s1; s++) {
      const n = M.slotNight[s];
      if (!n) continue;
      if (rest) restDots(n, (x, y) => dots.push(map.x(x), map.y(y), restR, 1 - e));
      if (fine && n !== focusN) fineDots(n, (x, y) => dots.push(map.x(x), map.y(y), FINE_R, e), every);
    }
    return dots;
  }

  /* Thousands of small round dots, quickly: each is stamped into a coverage buffer as an anti-aliased disc, and the
     buffer goes onto the canvas in one call. A software canvas takes seconds to fill that many arcs as paths. The
     canvas covers (ox, oy, w, h) in plot px; every dot is the nights' color. */
  let cov = null;
  function stamp(cv, ox, oy, w, h, dots) {
    const k = kit.dpr(), W = Math.max(1, Math.round(w * k)), H = Math.max(1, Math.round(h * k));
    if (cv.width !== W || cv.height !== H) { cv.width = W; cv.height = H; }
    if (!cov || cov.length < W * H) cov = new Float32Array(W * H);
    else cov.fill(0, 0, W * H);
    for (let i = 0; i < dots.length; i += 4) {
      const x = (dots[i] - ox) * k, y = (dots[i + 1] - oy) * k, R = dots[i + 2] * k, wt = dots[i + 3];
      if (wt <= 0) continue;
      const x0 = Math.max(0, Math.floor(x - R - 1)), x1 = Math.min(W - 1, Math.ceil(x + R + 1));
      const y0 = Math.max(0, Math.floor(y - R - 1)), y1 = Math.min(H - 1, Math.ceil(y + R + 1));
      for (let py = y0; py <= y1; py++) {
        const dy = py + 0.5 - y;
        for (let px = x0; px <= x1; px++) {
          const dx = px + 0.5 - x, c = R + 0.5 - Math.sqrt(dx * dx + dy * dy);
          if (c <= 0) continue;
          const a = (c >= 1 ? 1 : c) * wt, j = py * W + px;
          cov[j] += a * (1 - cov[j]);
        }
      }
    }
    const ctx = cv.getContext('2d');
    const img = ctx.createImageData(W, H), d = img.data;
    const P = tokens.rgb.primary, A = P[3] * NIGHT_ALPHA * 255;
    for (let j = 0, q = 0; j < W * H; j++, q += 4) {
      const a = cov[j];
      if (!a) continue;
      d[q] = P[0]; d[q + 1] = P[1]; d[q + 2] = P[2]; d[q + 3] = a * A;
    }
    ctx.putImageData(img, 0, 0);
  }

  /* The night under the glass: its minutes in ink, with a tick and its time at each end. */
  function paintFocus(ctx, map, e, focusN) {
    if (!focusN || e <= 0.001) return;
    const T = tokens, I = T.rgb.ink;
    const p = new Path2D();
    fineDots(focusN, (x, y) => disc(p, map.x(x), map.y(y), FOCUS_R), fineStep());
    ctx.fillStyle = kit.rgba(I, I[3] * e); ctx.fill(p);
    const x = map.x(X(focusN.slot));
    const ya = map.y(Y(focusN.a)) - 2.5, yw = map.y(Y(focusN.w)) + 2.5;
    ctx.strokeStyle = kit.rgba(I, I[3] * e); ctx.lineWidth = 1;
    ctx.beginPath(); ctx.moveTo(x - 3.5, ya); ctx.lineTo(x + 3.5, ya); ctx.moveTo(x - 3.5, yw); ctx.lineTo(x + 3.5, yw); ctx.stroke();
    const lastWake = focusN.spans.reduce((acc, s) => (s.wt > acc ? s.wt : acc), focusN.spans[0].wt);
    const left = map.flip && x > map.flip;
    ctx.font = `9.5px ${T.mono}`;
    ctx.textAlign = left ? 'right' : 'left'; ctx.textBaseline = 'middle';
    ctx.lineJoin = 'round'; ctx.lineWidth = 3; ctx.strokeStyle = kit.rgba(T.rgb.page, e);
    ctx.fillStyle = kit.rgba(I, I[3] * e);
    for (const [y, t] of [[ya, focusN.spans[0].at], [yw, lastWake]]) {
      if (y < map.top || y > map.bottom) continue;
      const text = kit.formatTime(t), tx = left ? x - 7 : x + 7;
      ctx.strokeText(text, tx, y); ctx.fillText(text, tx, y);
    }
  }

  /* Everything under the glass, drawn into ctx in plot px: the frame, the nights stamped over (ox, oy, w, h), then
     the night being read. */
  let lensDots = null;
  function paintLens(ctx, S, e, ox, oy, w, h) {
    paintFrame(ctx, S.map);
    if (!lensDots) lensDots = document.createElement('canvas');
    stamp(lensDots, ox, oy, w, h, nightDots(S.map, e, focus));
    ctx.drawImage(lensDots, ox, oy, w, h);
    paintFocus(ctx, S.map, e, focus);
  }

  /* At rest the nights are drawn once into their own canvas. The base is the frame and that canvas, so the frame's
     reveal costs a few copies a frame, with the nights fading in over the 48px behind its dot. */
  const IDENTITY = { x: (v) => v, y: (v) => v, mx: 1, my: 1, x0: -Infinity, x1: Infinity, flip: 0 };
  const RAMP = 48;
  let stripCv = null, stripDirty = true;
  function drawBase() {
    if (!width) return;
    const ctx = kit.fitCanvas(base, width, HEIGHT);
    baseDirty = false;
    if (!M.nights.length) return;
    const all = { ...IDENTITY, x0: B().left, x1: B().right };
    if (stripDirty || !stripCv) {
      if (!stripCv) stripCv = document.createElement('canvas');
      stamp(stripCv, 0, 0, width, HEIGHT, nightDots(all, 0, null));
      stripDirty = false;
    }
    paintFrame(ctx, all);
    const x1 = revealDraw;
    if (x1 >= width + RAMP) { ctx.drawImage(stripCv, 0, 0, width, HEIGHT); return; }
    const k = stripCv.width / width;
    const slice = (xa, xb, a) => {
      xa = Math.max(0, xa); xb = Math.min(width, xb);
      if (xb - xa < 0.01 || a <= 0) return;
      ctx.globalAlpha = a;
      ctx.drawImage(stripCv, xa * k, 0, (xb - xa) * k, stripCv.height, xa, 0, xb - xa, HEIGHT);
    };
    slice(0, x1 - RAMP, 1);
    for (let i = 0; i < 12; i++) slice(x1 - RAMP + (i * RAMP) / 12, x1 - RAMP + ((i + 1) * RAMP) / 12, 1 - (i + 0.5) / 12);
    ctx.globalAlpha = 1;
  }

  /* ---------- the glass ---------- */
  /* The point being read (fx, fy) sits at the glass's center, or off it inside the glass where the glass meets the
     edge of the plot, so the first night, the last and the ends of the clock can all be read. */
  function lensState(e) {
    const hw = lensHW(), hh = LENS_HH;
    const mx = 1 + (MX() - 1) * e, my = 1 + (MY - 1) * e;
    const cx = cxS.x, cy = cyS.x, fx = target.fx, fy = target.fy;
    /* 18px keeps the night and its times clear of the 14px rim, where the glass bends what is under it */
    const px = cx + kit.clamp((fx - cx) * mx, -(hw - 18), hw - 18);
    const py = cy + kit.clamp((fy - cy) * my, -(hh - 10), hh - 10);
    /* the strip that shows anywhere under the glass, from its left edge to its right, past the rim */
    const x0 = fx + (cx - hw - LENS_PAD - px) / mx, x1 = fx + (cx + hw + LENS_PAD - px) / mx;
    return {
      hw, hh, rad: hh, cx, cy, fx, px, mx, my,
      map: { x: (v) => px + (v - fx) * mx, y: (v) => py + (v - fy) * my, mx, my, x0, x1, flip: cx + hw * 0.3, top: cy - hh + 12, bottom: cy + hh - 12 },
    };
  }

  function lensPath(ctx, S, inset = 0) {
    const hw = S.hw - inset, hh = S.hh - inset, r = Math.max(0, Math.min(S.rad - inset, hw, hh));
    ctx.save();
    ctx.translate(S.cx, S.cy);
    if (stretch.x !== 1) { ctx.rotate(ang); ctx.scale(stretch.x, 1 / stretch.x); ctx.rotate(-ang); }
    ctx.beginPath();
    ctx.moveTo(-hw + r, -hh);
    ctx.lineTo(hw - r, -hh); ctx.arc(hw - r, -hh + r, r, -Math.PI / 2, 0);
    ctx.lineTo(hw, hh - r); ctx.arc(hw - r, hh - r, r, 0, Math.PI / 2);
    ctx.lineTo(-hw + r, hh); ctx.arc(-hw + r, hh - r, r, Math.PI / 2, Math.PI);
    ctx.lineTo(-hw, -hh + r); ctx.arc(-hw + r, -hh + r, r, Math.PI, Math.PI * 1.5);
    ctx.closePath();
    ctx.restore();
  }

  const dark = () => { const c = tokens.rgb.page; return (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) / 255 < 0.45; };

  function ensureGlass() {
    if (glass) return glass;
    if (!glFailed) {
      const cv = kit.h('canvas', 'vk-loupe-glass', { 'aria-hidden': 'true' });
      const G = makeGL(cv);
      if (G) {
        cv.style.pointerEvents = 'none';
        cv.addEventListener('webglcontextlost', (ev) => { ev.preventDefault(); glFailed = true; dropGlass(); loop.wake(); });
        parts.plot.appendChild(cv);
        glass = { kind: 'gl', cv, ...G };
        return glass;
      }
      glFailed = true;
    }
    const cv = kit.h('canvas', 'vk-loupe-glass', { 'aria-hidden': 'true' });
    cv.style.pointerEvents = 'none';
    parts.plot.appendChild(cv);
    glass = { kind: '2d', cv };
    return glass;
  }
  function dropGlass() {
    if (!glass) return;
    if (glass.kind === 'gl') { const ext = glass.gl.getExtension('WEBGL_lose_context'); if (ext && !glass.gl.isContextLost()) ext.loseContext(); }
    glass.cv.remove();
    glass = null;
  }
  function clearGlass() {
    if (!glass) return;
    if (glass.kind === 'gl') { const { gl } = glass; gl.viewport(0, 0, glass.cv.width, glass.cv.height); gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT); }
    else kit.fitCanvas(glass.cv, width, HEIGHT);
  }

  function drawGlass(e) {
    const G = ensureGlass();
    const S = lensState(e);
    const T = tokens, isDark = dark();
    if (G.kind === '2d') {
      const ctx = kit.fitCanvas(G.cv, width, HEIGHT);
      ctx.save();
      lensPath(ctx, S);
      ctx.shadowColor = isDark ? kit.rgba(T.rgb.background, 0.7 * e) : kit.rgba(T.rgb.ink, 0.16 * e);
      ctx.shadowBlur = 16; ctx.shadowOffsetY = 6;
      ctx.fillStyle = T.page; ctx.fill();
      ctx.restore();
      ctx.save();
      lensPath(ctx, S);
      ctx.clip();
      ctx.fillStyle = T.page;
      const bx = S.cx - S.hw - 2, by = S.cy - S.hh - 2, bw = S.hw * 2 + 4, bh = S.hh * 2 + 4;
      ctx.fillRect(bx, by, bw, bh);
      paintLens(ctx, S, e, bx, by, bw, bh);
      ctx.restore();
      lensPath(ctx, S);
      ctx.strokeStyle = kit.rgba(T.rgb.ink, T.rgb.ink[3] * 0.16 * e); ctx.lineWidth = 1; ctx.stroke();
      lensPath(ctx, S, 1.5);
      const g = ctx.createLinearGradient(S.cx - S.hw, S.cy - S.hh, S.cx, S.cy);
      const hi = isDark ? T.rgb.ink : T.rgb.background;
      g.addColorStop(0, kit.rgba(hi, (isDark ? 0.3 : 0.9) * e)); g.addColorStop(1, kit.rgba(hi, 0));
      ctx.strokeStyle = g; ctx.lineWidth = 1.2; ctx.stroke();
      return;
    }
    /* WebGL: the lens content is drawn in 2D past the rim, then refracted */
    const lw = 2 * (S.hw + LENS_PAD), lh = 2 * (S.hh + LENS_PAD);
    const ox = S.cx - lw / 2, oy = S.cy - lh / 2;
    if (!lensCv) lensCv = document.createElement('canvas');
    const lctx = kit.fitCanvas(lensCv, lw, lh);
    lctx.translate(-ox, -oy);
    lctx.fillStyle = T.page;
    lctx.fillRect(ox, oy, lw, lh);
    paintLens(lctx, S, e, ox, oy, lw, lh);
    const { gl, prog, u, tex, vao, cv } = G;
    const r = kit.dpr(), bw = Math.max(1, Math.round(width * r)), bh = Math.max(1, Math.round(HEIGHT * r));
    if (cv.width !== bw || cv.height !== bh) { cv.width = bw; cv.height = bh; }
    if (cv.style.width !== `${width}px`) cv.style.width = `${width}px`;
    if (cv.style.height !== `${HEIGHT}px`) cv.style.height = `${HEIGHT}px`;
    gl.viewport(0, 0, bw, bh);
    gl.clearColor(0, 0, 0, 0); gl.clear(gl.COLOR_BUFFER_BIT);
    gl.useProgram(prog);
    gl.bindVertexArray(vao);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, tex);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, false);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, lensCv);
    gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
    gl.uniform1i(u.uLens, 0);
    gl.uniform2f(u.uRes, bw, bh);
    gl.uniform1f(u.uS, bw / width);
    gl.uniform2f(u.uC, S.cx, S.cy);
    gl.uniform2f(u.uHalf, S.hw, S.hh);
    gl.uniform1f(u.uRad, S.rad);
    gl.uniform1f(u.uStr, e);
    gl.uniform1f(u.uAng, ang);
    gl.uniform1f(u.uStretch, stretch.x);
    gl.uniform2f(u.uLO, ox, oy);
    gl.uniform2f(u.uLS, lw, lh);
    const c3 = (c) => [c[0] / 255, c[1] / 255, c[2] / 255];
    gl.uniform3f(u.uInk, ...c3(T.rgb.ink));
    gl.uniform3f(u.uHi, ...c3(isDark ? T.rgb.ink : T.rgb.background));
    gl.uniform1f(u.uHiA, isDark ? 0.25 : 0.7);
    gl.uniform3f(u.uShC, ...c3(isDark ? T.rgb.background : T.rgb.ink));
    gl.uniform1f(u.uShA, isDark ? 0.5 : 0.07);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
  }

  /* ---------- the sentence beneath the glass, and the rows it hands over to ---------- */
  function setSaid(n) {
    if (n === saidFor && saidW === width) return;
    saidFor = n; saidW = width;
    if (!n) { said.textContent = ''; return; }
    const ctx = base.getContext('2d');
    ctx.save();
    ctx.font = `15px ${tokens.serif}`;
    const forms = sentences(n, timeWords(M));
    const text = forms.find((s) => ctx.measureText(s).width <= width - 8) || forms[forms.length - 1];
    said.__w = ctx.measureText(text).width;
    ctx.restore();
    if (said.textContent !== text) said.textContent = text;
  }
  /* While the glass is up the legend and the foot give way to the sentence. At rest their opacity is left to the
     kit's classes, which hold them back while the frame reveals the strip. */
  let lastE = -1;
  function placeSaid(e) {
    const w = said.__w || 0;
    const x = kit.clamp(cxS.x - w / 2, 0, Math.max(0, width - w));
    said.style.transform = `translateX(${x.toFixed(1)}px)`;
    if (Math.abs(e - lastE) > 0.002) {
      said.style.opacity = e.toFixed(3);
      const rest = e > 0.002 ? (1 - e).toFixed(3) : '';
      for (const node of [parts.readout, parts.foot]) {
        node.style.opacity = rest;
        if (!node.getAttribute('style')) node.removeAttribute('style');
      }
      lastE = e;
    }
  }
  function rowsWait(on) {
    for (const node of [parts.readout, parts.foot]) {
      node.classList.toggle('vk-fade', !on && !reduced());
      node.classList.toggle('vk-wait', on);
    }
  }

  /* ---------- the loupe's state ---------- */
  function nightAt(fx) {
    if (!M.nights.length) return null;
    const s = kit.clamp(Math.floor((fx - B().left) / slotW()), 0, M.nSlots - 1);
    return M.nearest[s];
  }
  /* A new press raises the glass where you press, even while the last one is still draining. */
  function activate() {
    if (!M.nights.length) return;
    if (!active) { cxS.x = target.cx; cxS.v = 0; cyS.x = target.cy; cyS.v = 0; stretch.x = 1; stretch.v = 0; }
    active = true;
    loop.wake();
  }
  function deactivate() {
    active = false; kbd = false; pending = false;
    loop.wake();
  }
  function aim(fx, fy) {
    const hw = lensHW(), b = B();
    target.fx = kit.clamp(fx, b.left, b.right);
    target.fy = kit.clamp(fy, b.top, b.bottom);
    target.cx = kit.clamp(target.fx, hw + 1, Math.max(hw + 1, width - hw - 1));
    target.cy = kit.clamp(target.fy, LENS_HH + 1, HEIGHT - LENS_HH - 1);
  }

  function step(dt) {
    clock += dt;
    const still = reduced();
    if (pending && clock >= pendingAt) { pending = false; activate(); }
    if (still) str = active ? 1 : 0;
    else str = active ? Math.min(1, str + dt / RISE) : Math.max(0, str - dt / DRAIN);
    let moving = pending;
    if (active || str > 0) {
      const px = cxS.x, py = cyS.x;
      if (still) { cxS.x = target.cx; cyS.x = target.cy; cxS.v = cyS.v = 0; }
      else { kit.springTo(cxS, target.cx, dt, 0.3, 0.08); kit.springTo(cyS, target.cy, dt, 0.3, 0.08); }
      const vx = (cxS.x - px) / Math.max(dt, 1e-3), vy = (cyS.x - py) / Math.max(dt, 1e-3), sp = Math.hypot(vx, vy);
      if (sp > 1) ang = Math.atan2(vy, vx);
      if (still) { stretch.x = 1; stretch.v = 0; } else kit.springTo(stretch, 1 + Math.min(0.12, sp * 0.0004), dt, 0.35, 0.1);
      if (active) focus = nightAt(target.fx);
      moving = moving || cxS.v !== 0 || cyS.v !== 0 || stretch.x !== 1 || (active ? str < 1 : str > 0);
    }
    if (mode === 'live' && revealDraw < width + RAMP) {
      revealDraw = reveal < Infinity ? reveal : still ? Infinity : revealDraw + 1400 * dt;
      baseDirty = true;
      moving = true;
    }
    if (baseDirty) drawBase();
    const e = kit.ease.out(str);
    if (str > 0.0005 && width) { drawGlass(e); wasShown = true; }
    else if (wasShown) { clearGlass(); wasShown = false; }
    setSaid(str > 0.0005 ? focus : null);
    placeSaid(str > 0.0005 ? e : 0);
    return moving;
  }

  /* ---------- the frame's reveal ---------- */
  function setReveal(x) {
    if (mode === 'none' && x === Infinity) return;
    if (mode === 'none') { mode = 'live'; revealDraw = x; }
    reveal = x;
    if (x < Infinity) revealDraw = x;
    rowsWait(reveal < Infinity);
    baseDirty = true;
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  function fromEvent(e) {
    const r = parts.plot.getBoundingClientRect();
    const touch = e.pointerType === 'touch';
    aim(e.clientX - r.left, e.clientY - r.top - (touch ? TOUCH_LIFT : 0));
  }
  const onDown = (e) => {
    if (e.button !== 0 || !M.nights.length || !width) return;
    ptr = e.pointerId;
    try { parts.plot.setPointerCapture(e.pointerId); } catch (err) { /* capture is a nicety */ }
    kbd = false;
    fromEvent(e);
    if (e.pointerType === 'touch') { pending = true; pendingAt = clock + TOUCH_HOLD; loop.wake(); }
    else activate();
  };
  const onMove = (e) => {
    if (ptr === null || e.pointerId !== ptr) return;
    fromEvent(e);
    if (active || pending) loop.wake();
  };
  const onUp = (e) => {
    if (ptr === null || e.pointerId !== ptr) return;
    ptr = null;
    deactivate();
  };
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerup', onUp);
  parts.plot.addEventListener('pointercancel', onUp);
  parts.plot.addEventListener('lostpointercapture', onUp);

  /* The keyboard starts the glass on the minute the night began; up and down then move it an hour along the clock. */
  function keyTo(i) {
    const n = M.nights[i];
    if (!n) return;
    if (!active || !kbd || keyY === null) keyY = Y(n.a) + 12;
    kbd = true;
    focus = n;
    aim(X(n.slot), keyY);
    activate();
  }
  const stopKeys = kit.keyNav(parts.plot, {
    count: () => M.nights.length,
    get: () => (active && focus ? focus.i : null),
    set: (i, why) => {
      if (i === null) { if (why === 'blur' && ptr !== null) return; deactivate(); keyY = null; return; }
      keyTo(i);
    },
    page: 7,
  });
  const onKey = (e) => {
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
    if (!active || !kbd || e.altKey || e.ctrlKey || e.metaKey) return;
    e.preventDefault();
    const b = B();
    keyY = kit.clamp((keyY === null ? target.fy : keyY) + (e.key === 'ArrowDown' ? 1 : -1) * 60 * pxPerMin(), b.top, b.bottom);
    aim(target.fx, keyY);
    loop.wake();
  };
  parts.plot.addEventListener('keydown', onKey);

  /* ---------- setup and the handle ---------- */
  function ariaLabel() {
    if (!M.nights.length) return 'Actogram with no rows to show.';
    const f = (d) => d.toLocaleDateString(undefined, { month: 'long', day: 'numeric', year: 'numeric' });
    return `Actogram of ${M.nouns[1]} from ${f(M.nights[0].date)} to ${f(M.nights[M.nights.length - 1].date)}, each ${M.nouns[0]} a column of dots from the interval's start to its end, on a clock from ${clockLabel(M.lo)} at the top to ${clockLabel(M.hi)} at the foot.`;
  }
  function applyStyle() {
    said.style.fontFamily = tokens.serif;
    said.style.color = tokens.ink;
  }
  function apply() {
    M = model(out);
    focus = null; saidFor = null; keyY = null;
    if (active) deactivate();
    kit.setLabel(parts.plot, ariaLabel());
    kit.setReadout(parts.readout, [{ label: M.nights.length ? `Each ${M.nouns[0]}, from start to end` : 'Nothing to show' }]);
    if (opts.foot === false) parts.foot.textContent = ''; else kit.foot(parts.foot, out, opts);
    if (mode === 'live') rowsWait(reveal < Infinity);
    baseDirty = true; stripDirty = true;
    loop.wake();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    baseDirty = true; stripDirty = true;
    if (active) aim(target.fx, target.fy);
    loop.wake();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; applyStyle(); baseDirty = true; stripDirty = true; saidFor = null; loop.wake(); }
  });
  const stopDpr = kit.watchDpr(() => { baseDirty = true; stripDirty = true; loop.wake(); });
  const stopMotion = kit.watchReducedMotion(() => loop.wake());
  /* The sentence is measured to pick the form that fits, so it is measured again once the serif has loaded. */
  const fonts = typeof document !== 'undefined' ? document.fonts : null;
  const onFonts = () => { saidFor = null; baseDirty = true; loop.wake(); };
  if (fonts) {
    if (fonts.addEventListener) fonts.addEventListener('loadingdone', onFonts);
    if (fonts.load) fonts.load(`15px ${tokens.serif}`).then(onFonts, () => {});
  }
  applyStyle();
  apply();
  if (mode === 'live') loop.wake();

  return {
    plot: parts.plot,
    update(next, patch) {
      if (patch) opts = { ...opts, ...patch };
      if (patch && patch.webgl === false && glass && glass.kind === 'gl') { glFailed = true; dropGlass(); }
      out = next || {};
      apply();
    },
    setReveal,
    get glass() { return glass ? glass.kind : null; },
    destroy() {
      loop.stop(); stopKeys(); stopWidth(); stopTheme(); stopDpr(); stopMotion();
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerup', onUp);
      parts.plot.removeEventListener('pointercancel', onUp);
      parts.plot.removeEventListener('lostpointercapture', onUp);
      parts.plot.removeEventListener('keydown', onKey);
      if (fonts && fonts.removeEventListener) fonts.removeEventListener('loadingdone', onFonts);
      dropGlass();
      root.remove();
    },
  };
}
