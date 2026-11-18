// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/path.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* path.js: a route map. A route drawn from location points in order, from where it started to where it ended, with
   the time at each end, for where a single movement went: a walk, a run, a drive, a day's travel. A new map mark:
   kind "map", mark "route".

   This is the drawing without tiles: the route laid on a faint dotted ground, the way the page draws it and the way
   the app can draw it when the box holds no map files. In the app with map files, ShowMap.svelte draws the same
   route as a dotted line layer on the box's atlas (charts/entries/path.json says how), and the geometry here (the
   projection, the nearest point on the route, the time at any point along it, where the end labels go) ports to it.
   - The points are joined in time order. The route is a line of primary dots 4.4px apart, from a ring at the start
     to a filled dot at the end, each labelled with its time in the chart's mono. Labels take the side of their mark
     that is clear of the route, of the plot's edge and of each other, so a loop that ends where it began stays
     readable.
   - The projection is Web Mercator, as the atlas uses, fitted to the route with one scale for both axes, north up.
     A scale bar sits at the bottom left, in feet and miles for a reader in the United States and meters and
     kilometers elsewhere.
   - Rest anywhere along the route: a dot follows the nearest point on it, and the readout gives the time there,
     read between the two points around it, with the time and distance from the start. Tab to the map and use the
     left and right arrow keys to step through the points; Home and End jump to the ends and Escape clears.
   - In the frame the route draws itself from the start as the frame's sweep crosses the plot, so the movement replays
     once in the time the chart takes to arrive. Under reduced motion it is there at once.

   Columns, found by name: lat, lon and at (latitude, longitude and occurred_at work too). 2 to 199 rows.

   Contract: see the top of kit.js. Block: readout 26 + plot 260 + foot 26 = 312px. The plot is 260px, the height of
   ShowMap's map today, so a map result keeps its height whether it draws tiles or not. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'path', kinds: ['map'], marks: ['route'] });

const HEIGHT = 260;
const PAD = Object.freeze({ top: 8, right: 8, bottom: 8, left: 8 });
const FIT = Object.freeze({ top: 24, right: 32, bottom: 40, left: 32 });   /* room for the end labels and the scale bar */
const STEP = 4.4, DOT_R = 1.15;
const START_R = 3.6, END_R = 3.4;
const GRID = 13, GRID_R = 0.7;
const HIT = 28;                /* px from the route that still reads it */
const EARTH = 6378137;
const MIN_MPP = 0.6;           /* the closest the fit zooms: 0.6 m to a pixel */
const MAX_ROWS = 199;
const TAU = Math.PI * 2;
const D2R = Math.PI / 180;
const HINT = 'Use the left and right arrow keys to step through the points along the route.';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- units ---------- */

const imperial = () => {
  const lang = (typeof navigator !== 'undefined' && (navigator.language || '')) || '';
  return /^(en-US|en-LR|my)\b/i.test(lang) || lang === 'en';
};
function fmtDistance(m) {
  if (imperial()) {
    const mi = m / 1609.344;
    if (mi < 0.1) return `${Math.max(10, Math.round(m * 3.28084 / 10) * 10)} ft`;
    return `${mi < 10 ? mi.toFixed(1) : Math.round(mi)} mi`;
  }
  if (m < 1000) return `${Math.max(10, Math.round(m / 10) * 10)} m`;
  return `${m < 10000 ? (m / 1000).toFixed(1) : Math.round(m / 1000)} km`;
}
function fmtElapsed(ms) {
  const total = Math.round(ms / 60000);
  if (total < 60) return `${total} min`;
  const h = Math.floor(total / 60), m = total % 60;
  return m ? `${h} h ${m} m` : `${h} h`;
}
/* A scale bar's length: the longest round distance that fits in `px` at `mpp` meters a pixel. */
function scaleFor(px, mpp) {
  const ft = imperial();
  const unit = ft ? 0.3048 : 1;
  const n = (v) => v.toLocaleString(undefined, { maximumFractionDigits: 2 });
  const steps = ft
    ? [10, 20, 50, 100, 200, 500, 1000, 2000].map((f) => [f * unit, `${n(f)} ft`]).concat([0.25, 0.5, 1, 2, 5, 10, 20, 50, 100].map((mi) => [mi * 1609.344, `${n(mi)} mi`]))
    : [5, 10, 20, 50, 100, 200, 500].map((m) => [m, `${n(m)} m`]).concat([1, 2, 5, 10, 20, 50, 100].map((k) => [k * 1000, `${n(k)} km`]));
  let best = steps[0];
  for (const s of steps) if (s[0] / mpp <= px) best = s;
  return { px: best[0] / mpp, label: best[1] };
}

const fmtDay = (t) => t.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' });
const sameDay = (a, b) => a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();

function haversine(a, b) {
  const dLat = (b.lat - a.lat) * D2R, dLon = (b.lon - a.lon) * D2R;
  const s = Math.sin(dLat / 2) ** 2 + Math.cos(a.lat * D2R) * Math.cos(b.lat * D2R) * Math.sin(dLon / 2) ** 2;
  return 2 * EARTH * Math.asin(Math.min(1, Math.sqrt(s)));
}

/* ---------- the model ---------- */

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const find = (names) => names.find((n) => columns.includes(n)) || null;
  const latK = find(['lat', 'latitude']), lonK = find(['lon', 'longitude', 'lng']), atK = find(['at', 'occurred_at', 'time']);
  const num = (v) => (typeof v === 'number' ? v : typeof v === 'string' && v.trim() ? Number(v) : NaN);
  let pts = [];
  if (latK && lonK && atK) {
    pts = rows.map((r) => ({ lat: num(r[latK]), lon: num(r[lonK]), t: kit.parseTime(r[atK]) }))
      .filter((p) => Number.isFinite(p.lat) && Number.isFinite(p.lon) && Math.abs(p.lat) <= 85 && Math.abs(p.lon) <= 180 && p.t)
      .sort((a, b) => a.t - b.t)
      .slice(0, MAX_ROWS);
  }
  const cum = [0];
  for (let i = 1; i < pts.length; i++) cum.push(cum[i - 1] + haversine(pts[i - 1], pts[i]));
  for (const p of pts) { p.mx = p.lon * D2R; p.my = Math.log(Math.tan(Math.PI / 4 + (p.lat * D2R) / 2)); }
  return { columns, pts, cum, total: cum[cum.length - 1] || 0 };
}

function ariaLabel(M) {
  const P = M.pts;
  if (P.length < 2) return 'Route map, with too few points to draw.';
  const a = P[0].t, b = P[P.length - 1].t;
  const when = sameDay(a, b)
    ? `on ${fmtDay(a)}, from ${kit.formatTime(a)} to ${kit.formatTime(b)}`
    : `from ${fmtDay(a)}, ${kit.formatTime(a)} to ${fmtDay(b)}, ${kit.formatTime(b)}`;
  const ends = haversine(P[0], P[P.length - 1]);
  const loop = ends < Math.max(150, M.total * 0.08) ? ` It ends ${fmtDistance(ends)} from where it started.` : '';
  return `Route map with north up, ${when}, about ${fmtDistance(M.total)} from your location points. A ring marks the start and a dot the end.${loop}`;
}

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-path');
  el.appendChild(root);
  const parts = kit.block(root, { plotHeight: HEIGHT, readout: true, foot: opts.foot !== false, hint: HINT });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  parts.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let G = null;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;    /* { i, f }: a point f of the way from point i to point i + 1 */
  const ring = { x: 1, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let clock = 0, revealAt = 0, endAt = null;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- layout: the projection, the dots along the route, the labels and the scale ---------- */
  function layout() {
    G = null;
    const P = M.pts;
    if (!width || P.length < 2) return;
    const b = box(width);
    const fx0 = b.left + FIT.left, fx1 = Math.max(fx0 + 10, b.right - FIT.right);
    const fy0 = b.top + FIT.top, fy1 = Math.max(fy0 + 10, b.bottom - FIT.bottom);
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const p of P) { minX = Math.min(minX, p.mx); maxX = Math.max(maxX, p.mx); minY = Math.min(minY, p.my); maxY = Math.max(maxY, p.my); }
    const latC = (P.reduce((s, p) => s + p.lat, 0) / P.length) * D2R;
    const perUnit = EARTH * Math.cos(latC);              /* ground meters in one radian of Mercator x, here */
    const kMax = perUnit / MIN_MPP;
    const k = Math.min(kMax, (fx1 - fx0) / Math.max(1e-12, maxX - minX), (fy1 - fy0) / Math.max(1e-12, maxY - minY));
    const cx = (fx0 + fx1) / 2, cy = (fy0 + fy1) / 2, mcx = (minX + maxX) / 2, mcy = (minY + maxY) / 2;
    /* north up: a larger Mercator y is further north, so it is drawn higher */
    const S = P.map((p) => [cx + (p.mx - mcx) * k, cy - (p.my - mcy) * k]);
    const len = [0];
    for (let i = 1; i < S.length; i++) len.push(len[i - 1] + Math.hypot(S[i][0] - S[i - 1][0], S[i][1] - S[i - 1][1]));
    const total = len[len.length - 1];
    const dots = [];
    for (let s = 0, i = 0; s <= total; s += STEP) {
      while (i < S.length - 2 && len[i + 1] < s) i++;
      const segL = len[i + 1] - len[i] || 1, f = kit.clamp((s - len[i]) / segL, 0, 1);
      dots.push([S[i][0] + (S[i + 1][0] - S[i][0]) * f, S[i][1] + (S[i + 1][1] - S[i][1]) * f, s]);
    }
    const mpp = perUnit / k;
    const scale = scaleFor(Math.min(110, (b.right - b.left) * 0.28), mpp);
    G = { b, S, len, total, dots, mpp, scale, labels: null };
    G.labels = placeLabels();
  }

  /* Each end's time goes on the side of its mark that is clear of the route, the edges and the other label. */
  function placeLabels() {
    const ctx = canvas.getContext('2d');
    ctx.font = `9.5px ${tokens.mono}`;
    const P = M.pts, S = G.S, b = G.b, n = S.length;
    const along = (from, dir) => {
      /* the route's direction from an end, looked at about 24px along it */
      let j = from;
      while (j + dir >= 0 && j + dir < n && Math.hypot(S[j + dir][0] - S[from][0], S[j + dir][1] - S[from][1]) < 24) j += dir;
      if (j + dir >= 0 && j + dir < n) j += dir;
      const dx = S[j][0] - S[from][0], dy = S[j][1] - S[from][1], l = Math.hypot(dx, dy) || 1;
      return [dx / l, dy / l];
    };
    const ends = [
      { at: 0, text: kit.formatTime(P[0].t), toward: along(0, 1), r: START_R },
      { at: n - 1, text: kit.formatTime(P[n - 1].t), toward: along(n - 1, -1), r: END_R },
    ];
    const DIRS = [[1, 0], [-1, 0], [0, -1], [0, 1], [0.71, -0.71], [-0.71, -0.71], [0.71, 0.71], [-0.71, 0.71]];
    /* each end's candidate spots, each scored on its own: off the plot, over the route or over the other mark */
    const options = ends.map((e) => {
      const [mx, my] = S[e.at];
      const w = ctx.measureText(e.text).width, h = 10;
      const list = [];
      for (const far of [0, 7]) {
        for (const [dx, dy] of DIRS) {
          const off = e.r + 6 + far;
          const ax = mx + dx * off, ay = my + dy * off;
          const x0 = dx > 0.3 ? ax : dx < -0.3 ? ax - w : ax - w / 2;
          const y0 = dy > 0.3 ? ay : dy < -0.3 ? ay - h : ay - h / 2;
          const c = { x0, y0, x1: x0 + w, y1: y0 + h, text: e.text };
          let score = 3 * (dx * e.toward[0] + dy * e.toward[1]) + far * 0.4;
          if (c.x0 < b.left + 2 || c.x1 > b.right - 2 || c.y0 < b.top + 2 || c.y1 > b.bottom - 22) score += 1000;
          for (const d of G.dots) if (d[0] > c.x0 - 4 && d[0] < c.x1 + 4 && d[1] > c.y0 - 4 && d[1] < c.y1 + 4) score += 25;
          for (const q of ends) {
            if (q === e) continue;
            const [ox, oy] = S[q.at];
            if (ox > c.x0 - q.r - 4 && ox < c.x1 + q.r + 4 && oy > c.y0 - q.r - 4 && oy < c.y1 + q.r + 4) score += 400;
          }
          c.score = score;
          list.push(c);
        }
      }
      return list;
    });
    /* then the pair that scores best together, so two ends close to each other share the space */
    let best = null;
    for (const a of options[0]) for (const c of options[1]) {
      let score = a.score + c.score;
      if (a.x0 < c.x1 + 6 && a.x1 > c.x0 - 6 && a.y0 < c.y1 + 4 && a.y1 > c.y0 - 4) score += 500;
      if (!best || score < best.score) best = { score, pair: [a, c] };
    }
    return best.pair;
  }

  /* where a hover sits, in plot px, and the moment and distance it stands for */
  function at(hv) {
    const P = M.pts, S = G.S;
    const i = hv.i, j = Math.min(P.length - 1, i + 1), f = hv.f;
    const x = S[i][0] + (S[j][0] - S[i][0]) * f, y = S[i][1] + (S[j][1] - S[i][1]) * f;
    const t = new Date(P[i].t.getTime() + (P[j].t.getTime() - P[i].t.getTime()) * f);
    const d = M.cum[i] + (M.cum[j] - M.cum[i]) * f;
    return { x, y, t, d };
  }

  /* ---------- drawing ---------- */
  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const T = tokens, b = box(width);
    const live = mode === 'live';
    const groundA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;

    /* the faint dotted ground */
    if (groundA > 0) {
      const ground = new Path2D();
      const gx0 = b.left + ((b.right - b.left) % GRID) / 2, gy0 = b.top + ((b.bottom - b.top) % GRID) / 2;
      for (let y = gy0; y <= b.bottom + 0.1; y += GRID) for (let x = gx0; x <= b.right + 0.1; x += GRID) { ground.moveTo(x + GRID_R, y); ground.arc(x, y, GRID_R, 0, TAU); }
      ctx.globalAlpha = groundA;
      ctx.fillStyle = kit.rgba(T.rgb.borderStrong, T.rgb.borderStrong[3] * 0.85);
      ctx.fill(ground);
      ctx.globalAlpha = 1;
    }
    if (!G) return;
    const p = !live || reduced() || reveal === Infinity ? 1 : kit.clamp((reveal - b.left) / Math.max(1, b.right - b.left), 0, 1);
    const shown = p * G.total;
    const col = T.rgb.s0;

    /* the scale bar, faint, at the bottom left */
    ctx.save();
    ctx.globalAlpha = groundA;
    const sx = b.left + 10, sy = b.bottom - 10;
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(sx, sy, G.scale.px, 1);
    ctx.fillRect(sx, sy - 3, 1, 4);
    ctx.fillRect(sx + G.scale.px - 1, sy - 3, 1, 4);
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'alphabetic';
    ctx.fillText(G.scale.label, sx + G.scale.px + 6, sy + 3);
    ctx.restore();

    /* the route, a line of dots, drawn as far as the frame's sweep has come */
    const route = new Path2D();
    for (const d of G.dots) { if (d[2] > shown) break; route.moveTo(d[0] + DOT_R, d[1]); route.arc(d[0], d[1], DOT_R, 0, TAU); }
    ctx.fillStyle = kit.rgba(col, col[3] * (hover !== null ? 0.75 : 1));
    ctx.fill(route);
    if (p < 1 && shown > 0) {
      /* the head of the route while it draws */
      let i = 0;
      while (i < G.len.length - 2 && G.len[i + 1] < shown) i++;
      const f = kit.clamp((shown - G.len[i]) / (G.len[i + 1] - G.len[i] || 1), 0, 1);
      const hx = G.S[i][0] + (G.S[i + 1][0] - G.S[i][0]) * f, hy = G.S[i][1] + (G.S[i + 1][1] - G.S[i][1]) * f;
      ctx.fillStyle = kit.rgba(col); ctx.beginPath(); ctx.arc(hx, hy, 2.6, 0, TAU); ctx.fill();
    }

    /* the ends: a ring at the start, a dot at the end, each with its time */
    const [s0x, s0y] = G.S[0], [s1x, s1y] = G.S[G.S.length - 1];
    if (shown > 0 || p >= 1) {
      ctx.save();
      ctx.lineWidth = 1.3;
      ctx.fillStyle = T.page;
      ctx.strokeStyle = kit.rgba(col);
      ctx.beginPath(); ctx.arc(s0x, s0y, START_R, 0, TAU); ctx.fill(); ctx.stroke();
      ctx.restore();
    }
    const endG = p < 1 ? 0 : !live || reduced() || endAt === null ? 1 : kit.clamp(kit.stepSpring(clock - endAt, 0.32, 0.25), 0, 1.2);
    if (endG > 0) { ctx.fillStyle = kit.rgba(col); ctx.beginPath(); ctx.arc(s1x, s1y, END_R * endG, 0, TAU); ctx.fill(); }
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'alphabetic';
    const labelA = [shown > 0 || p >= 1 ? groundA : 0, p >= 1 ? (!live || reduced() || endAt === null ? 1 : kit.seg(clock - endAt, 0.05, 0.3)) : 0];
    G.labels.forEach((l, k) => {
      if (labelA[k] <= 0) return;
      ctx.globalAlpha = labelA[k];
      ctx.fillText(l.text, l.x0, l.y1 - 1.5);
    });
    ctx.restore();

    /* the dot you rest on, with its ring */
    if (hover !== null && p >= 1) {
      const a = at(hover);
      ctx.fillStyle = kit.rgba(col);
      ctx.beginPath(); ctx.arc(a.x, a.y, 3, 0, TAU); ctx.fill();
      ctx.save();
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(col, col[3] * 0.9);
      ctx.beginPath(); ctx.arc(a.x, a.y, 4.5 + 3 * ring.x, 0, TAU); ctx.stroke();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    if (!parts.readout) return;
    const P = M.pts;
    if (P.length < 2) { kit.setReadout(parts.readout, []); return; }
    if (hover === null || !G) {
      const a = P[0].t, b = P[P.length - 1].t;
      const span = sameDay(a, b)
        ? [{ when: fmtDay(a) }, { label: '', value: `${kit.formatTime(a)} to ${kit.formatTime(b)}` }]
        : [{ when: `${fmtDay(a)} to ${fmtDay(b)}` }, { label: '', value: `${kit.formatTime(a)} to ${kit.formatTime(b)}` }];
      kit.setReadout(parts.readout, span.concat([{ label: '', value: fmtDistance(M.total) }]));
      return;
    }
    const a = at(hover);
    kit.setReadout(parts.readout, [{ when: kit.formatTime(a.t) }, { label: 'From the start', value: `${fmtElapsed(a.t - P[0].t)}, ${fmtDistance(a.d)}` }]);
  }

  function setHover(h, from) {
    const same = h === hover || (h && hover && h.i === hover.i && Math.abs(h.f - hover.f) < 1e-3);
    if (same) { hoverFrom = from; return; }
    const was = hover;
    hover = h; hoverFrom = from;
    if (h !== null && was === null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while the route draws, its end lands, or a ring grows ---------- */
  function step(dt) {
    clock += dt;
    let moving = false;
    if (reduced()) { ring.x = 1; ring.v = 0; }
    else { kit.springTo(ring, 1, dt, 0.28, 0.15); moving = ring.x !== 1; }
    if (mode === 'live' && !reduced()) {
      if (clock - revealAt < 0.35 || reveal < Infinity) moving = true;
      if (endAt !== null && clock - endAt < 0.9) moving = true;
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function rowsWait(on) {
    for (const node of [parts.readout, parts.foot]) {
      if (!node) continue;
      node.classList.toggle('vk-fade', !on && !reduced());
      node.classList.toggle('vk-wait', on);
    }
  }
  function setReveal(x) {
    if (mode === 'none' && x === Infinity) return;
    if (mode === 'none') { mode = 'live'; revealAt = clock; }
    reveal = x;
    if (reveal === Infinity && endAt === null) endAt = clock;
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  function nearest(px, py) {
    if (!G) return null;
    const S = G.S;
    let best = null, dist = Infinity;
    for (let i = 0; i < S.length - 1; i++) {
      const [ax, ay] = S[i], [bx, by] = S[i + 1];
      const dx = bx - ax, dy = by - ay, l2 = dx * dx + dy * dy;
      const f = l2 ? kit.clamp(((px - ax) * dx + (py - ay) * dy) / l2, 0, 1) : 0;
      const qx = ax + dx * f - px, qy = ay + dy * f - py, d = qx * qx + qy * qy;
      if (d < dist) { dist = d; best = { i, f }; }
    }
    return best && Math.sqrt(dist) <= HIT ? best : null;
  }
  const pick = (e) => { const r = canvas.getBoundingClientRect(); return nearest(e.clientX - r.left, e.clientY - r.top); };
  const ready = () => !!G && (mode === 'none' || reveal === Infinity);
  const onMove = (e) => {
    if (!ready()) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    const h = pick(e);
    if (h === null && e.pointerType === 'touch') return;
    setHover(h, h === null ? null : e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!ready()) return;
    const h = pick(e);
    if (e.pointerType === 'touch' && (h === null || (hoverFrom === 'touch' && hover && hover.i === h.i && Math.abs(hover.f - h.f) < 0.5))) { setHover(null, null); return; }
    setHover(h, h === null ? null : e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);
  const toIndex = (h) => (h === null ? null : Math.round(h.i + h.f));
  const fromIndex = (k) => {
    const n = M.pts.length;
    return k === null ? null : k >= n - 1 ? { i: n - 2, f: 1 } : { i: k, f: 0 };
  };
  const stopKeys = kit.keyNav(parts.plot, {
    count: () => (ready() ? M.pts.length : 0),
    get: () => toIndex(hover),
    set: (k, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(fromIndex(k), 'key');
    },
    page: 10,
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
    readout();
    draw();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; layout(); draw(); }
  });
  const stopDpr = kit.watchDpr(() => draw());
  const stopMotion = kit.watchReducedMotion(() => { rowsWait(mode === 'live' && reveal < Infinity); loop.wake(); });
  apply();
  if (mode === 'live') { revealAt = clock; loop.wake(); }

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
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointerleave', onLeave);
      root.remove();
    },
  };
}
