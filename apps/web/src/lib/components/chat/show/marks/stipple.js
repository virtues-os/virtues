// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/stipple.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* stipple.js: Dot density map. kind "map", mark "density".

   Time spent across places, drawn as dots on a map: one dot for each ten minutes, scattered around each place, so
   the size of each pool shows where the hours went. It fits any stretch of time (a month, a season, a trip) and any
   set of places with the minutes spent at each, by named place or by the cells of a small grid. Places too far
   apart to share one map belong in a bar list instead.
   - A pool's radius grows with the square root of its dots, so every pool has the same density and its area is its
     time. The density is fixed, so a week draws smaller pools than a month, until the largest pool would be wider
     than about a seventh of the plot; then the dots pack closer and get finer. The dots sit on a sunflower spiral
     with jitter, thickest in the middle and thinning to nothing at the edge, seeded from the row's own coordinates,
     so a pool keeps its shape across redraws.
   - One dot is ten minutes. A result that would need more than 6,000 dots (a year, say) makes a dot stand for 20 or
     30 minutes or an hour, and the readout says so.
   - This is the drawing without tiles: Web Mercator fitted to the places with one scale for both axes, north up, on
     a faint ground of dots 13px apart, with any water as a halftone band, a scale bar, and each pool named beside it
     where there is room. In the app with map files, ShowMap.svelte lays the same dots on the box's atlas
     (charts/entries/stipple.json says how), and the dot layout, the pools and the label rule here port to it.
   - Pools that touch are read as one, up to the size of the largest single pool, so a query grouped by a small grid
     still reads as places. Rest on a pool, or tab to the map and use the left and right arrow keys to go from west
     to east: the other pools dim, a ring grows around it, and the readout gives its name and its time in words,
     such as "about 44 hours in October".
   - In the frame each pool fills from its middle outward as the frame's sweep passes it, so the time seems to collect
     there. Under reduced motion it is there at once. Nothing runs while nothing moves.

   Columns, found by name: lat, lon and minutes, with an optional label naming the place (latitude, longitude, lng,
   duration_minutes, mins, name, place_name, place and title work too). Up to 400 rows. Two optional fields on the
   result: period, a few words for the stretch the minutes cover ("this month", "in October"), and water, lines of
   [lat, lon] drawn as a halftone band. show.rs passes period through from the call; water is only for a drawing
   without tiles, and in the app the atlas draws the water.

   Contract: see the top of kit.js. Block: readout 26 + plot 260 + foot 26 = 312px. The plot is 260px, the height of
   ShowMap's map today, so a map result keeps its height whether it draws tiles or not. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'stipple', kinds: ['map'], marks: ['density'] });

const HEIGHT = 260;
const PAD = Object.freeze({ top: 8, right: 8, bottom: 8, left: 8 });
const GRID = 13, GRID_R = 0.7;
const UNITS = Object.freeze([10, 20, 30, 60, 120, 240, 480, 1440]);   /* minutes a dot stands for */
const MAX_DOTS = 6000;
const MAX_ROWS = 400;
const HIT = 10;                /* px outside a pool that still reads it */
const RU = 1.8;                /* px of radius per square root of a dot, unless the largest pool would be too big */
const EARTH = 6378137;
const MIN_MPP = 2;             /* the closest the fit zooms: 2 m to a pixel */
const WATER_HW = 3.8, WATER_STEP = 2.3, WATER_R = 0.62;
const LABEL_FONT = 9.5;
const TAU = Math.PI * 2;
const D2R = Math.PI / 180;
const GOLDEN = Math.PI * (3 - Math.sqrt(5));
const HINT = 'Use the left and right arrow keys to read each place, from west to east.';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

/* ---------- words ---------- */

const imperial = () => {
  const lang = (typeof navigator !== 'undefined' && (navigator.language || '')) || '';
  return /^(en-US|en-LR|my)\b/i.test(lang) || lang === 'en';
};
/* A scale bar's length: the longest round distance that fits in `px` at `mpp` meters a pixel. */
function scaleFor(px, mpp) {
  const unit = imperial() ? 0.3048 : 1;
  const n = (v) => v.toLocaleString(undefined, { maximumFractionDigits: 2 });
  const steps = imperial()
    ? [10, 20, 50, 100, 200, 500, 1000, 2000].map((f) => [f * unit, `${n(f)} ft`]).concat([0.25, 0.5, 1, 2, 5, 10, 20, 50, 100].map((mi) => [mi * 1609.344, `${n(mi)} mi`]))
    : [5, 10, 20, 50, 100, 200, 500].map((m) => [m, `${n(m)} m`]).concat([1, 2, 5, 10, 20, 50, 100].map((k) => [k * 1000, `${n(k)} km`]));
  let best = steps[0];
  for (const s of steps) if (s[0] / mpp <= px) best = s;
  return { px: best[0] / mpp, label: best[1] };
}

/* A time in words, rounded the way a person says it: about 40 minutes, about an hour and a half, about 44 hours. */
function aboutTime(min) {
  if (min < 55) return `about ${Math.max(5, Math.round(min / 5) * 5)} minutes`;
  if (min < 75) return 'about an hour';
  const h = min / 60;
  if (h < 10) {
    const r = Math.round(h * 2) / 2;
    if (r === 1.5) return 'about an hour and a half';
    return r % 1 ? `about ${Math.floor(r)} and a half hours` : `about ${r} hours`;
  }
  return `about ${kit.formatNumber(Math.round(h))} hours`;
}
const unitWords = (u) => (u < 60 ? `${u} minutes` : u === 60 ? 'an hour' : u < 1440 ? `${u / 60} hours` : 'a day');
const capital = (s) => (s ? s.charAt(0).toUpperCase() + s.slice(1) : s);
const UNNAMED = 'A place without a name';

/* ---------- the model ---------- */

function hash(s) {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
  return h >>> 0;
}
function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6D2B79F5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const mercY = (lat) => Math.log(Math.tan(Math.PI / 4 + (lat * D2R) / 2));
const inRange = (lat, lon) => Number.isFinite(lat) && Number.isFinite(lon) && Math.abs(lat) <= 85 && Math.abs(lon) <= 180;

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const find = (names) => names.find((n) => columns.includes(n)) || null;
  const latK = find(['lat', 'latitude']), lonK = find(['lon', 'longitude', 'lng']);
  const minK = find(['minutes', 'duration_minutes', 'mins']);
  const labK = find(['label', 'name', 'place_name', 'place', 'title']);
  const num = (v) => (typeof v === 'number' ? v : typeof v === 'string' && v.trim() ? Number(v) : NaN);
  let places = [];
  if (latK && lonK && minK) {
    places = rows.slice(0, MAX_ROWS).map((r) => ({
      lat: num(r[latK]), lon: num(r[lonK]), minutes: num(r[minK]),
      label: labK && typeof r[labK] === 'string' && r[labK].trim() ? r[labK].trim() : null,
    })).filter((p) => inRange(p.lat, p.lon) && Number.isFinite(p.minutes) && p.minutes > 0);
  }
  const total = places.reduce((s, p) => s + p.minutes, 0);
  let unit = UNITS[UNITS.length - 1];
  for (const u of UNITS) if (places.reduce((s, p) => s + Math.round(p.minutes / u), 0) <= MAX_DOTS) { unit = u; break; }
  for (const p of places) {
    p.n = Math.round(p.minutes / unit);
    p.mx = p.lon * D2R; p.my = mercY(p.lat);
    p.seed = hash(`${p.lat.toFixed(5)},${p.lon.toFixed(5)},${p.label || ''}`);
  }
  const drawn = places.filter((p) => p.n > 0);
  const water = [];
  if (out && Array.isArray(out.water)) {
    for (const line of out.water.slice(0, 20)) {
      if (!Array.isArray(line)) continue;
      const pts = line.slice(0, 2000).map((q) => (Array.isArray(q) ? [num(q[0]), num(q[1])] : [NaN, NaN])).filter(([la, lo]) => inRange(la, lo));
      if (pts.length > 1) water.push(pts.map(([la, lo]) => [lo * D2R, mercY(la)]));
    }
  }
  const period = out && typeof out.period === 'string' ? out.period.trim().replace(/\s+/g, ' ').slice(0, 40) : '';
  return { columns, places, drawn, total, unit, water, period, nMax: Math.max(1, ...drawn.map((p) => p.n)) };
}

function ariaLabel(M, pools) {
  if (!M.drawn.length) return 'Map of where your time went, with no places to draw.';
  const when = M.period ? ` ${M.period}` : '';
  const named = [...pools].sort((a, b) => b.minutes - a.minutes);
  const list = named.slice(0, 6).map((p) => `${p.label || UNNAMED}, ${aboutTime(p.minutes)}`).join('; ');
  const more = named.length > 6 ? `; and ${named.length - 6} more` : '';
  return `Map of where your time went${when}, north up, with one dot for every ${unitWords(M.unit)} you spent at a place: ${list}${more}.`;
}

/* ---------- the component ---------- */

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-stipple');
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
  let hover = null, hoverFrom = null;     /* the index of a pool in G.pools, west to east */
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let clock = 0, revealAt = 0;
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- layout: the projection, the pools and their dots, the ground, the labels ---------- */
  function layout() {
    G = null;
    const P = M.drawn;
    if (!width || !P.length) return;
    const b = box(width);
    const W = b.right - b.left, H = b.bottom - b.top;
    const rMax = kit.clamp(Math.sqrt(W * H) * 0.13, 14, 44);
    const ru = Math.min(RU, rMax / Math.sqrt(M.nMax));     /* px of radius per square root of a dot */
    const spacing = ru * Math.sqrt(Math.PI);               /* the mean distance between dots at that density */
    const dotR = kit.clamp(spacing * 0.27, 0.5, 1.15);
    /* room around the fit for the largest pool, the names and the scale bar */
    const mx = Math.min(W * 0.24, rMax * 0.55 + 30), mt = Math.min(H * 0.24, rMax * 0.55 + 14), mb = Math.min(H * 0.26, rMax * 0.55 + 24);
    const fx0 = b.left + mx, fx1 = Math.max(fx0 + 10, b.right - mx), fy0 = b.top + mt, fy1 = Math.max(fy0 + 10, b.bottom - mb);
    let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
    for (const p of P) { minX = Math.min(minX, p.mx); maxX = Math.max(maxX, p.mx); minY = Math.min(minY, p.my); maxY = Math.max(maxY, p.my); }
    const latC = (P.reduce((s, p) => s + p.lat * p.minutes, 0) / P.reduce((s, p) => s + p.minutes, 0)) * D2R;
    const perUnit = EARTH * Math.cos(latC);                /* ground meters in one radian of Mercator x, here */
    const k = Math.min(perUnit / MIN_MPP, (fx1 - fx0) / Math.max(1e-12, maxX - minX), (fy1 - fy0) / Math.max(1e-12, maxY - minY));
    const cx = (fx0 + fx1) / 2, cy = (fy0 + fy1) / 2, mcx = (minX + maxX) / 2, mcy = (minY + maxY) / 2;
    /* north up: a larger Mercator y is further north, so it is drawn higher */
    const project = (x, y) => [cx + (x - mcx) * k, cy - (y - mcy) * k];
    P.forEach((p, i) => { p.ix = i; });
    for (const p of P) {
      [p.x, p.y] = project(p.mx, p.my);
      p.R = Math.max(1.2, ru * Math.sqrt(p.n));
      p.dots = dotsFor(p, spacing);
      p.fill = 0.35 + 0.6 * Math.sqrt(p.n / M.nMax);       /* seconds a pool takes to fill as it arrives */
    }
    const pools = poolsOf(P, rMax);
    const water = M.water.map((line) => line.map(([x, y]) => project(x, y)));
    const mpp = perUnit / k;
    const scale = scaleFor(Math.min(110, W * 0.28), mpp);
    G = { b, P, pools, dotR, water, mpp, scale, ground: null, halftone: null, labels: [] };
    G.halftone = halftone(b, water);
    G.ground = ground(b, water);
    G.labels = placeLabels();
    if (mode === 'live') markBorn();
  }

  /* A pool's dots, as offsets from its place: a sunflower spiral with jitter, whose density falls as (1 - r²)^0.6, so
     a pool is thickest in the middle and thins to nothing at its edge. u is the share of the pool's dots nearer the
     middle than this one; a pool fills in that order. */
  function dotsFor(p, spacing) {
    const r = rng(p.seed), n = p.n, out = new Float32Array(n * 3);
    const a0 = r() * TAU;
    for (let i = 0; i < n; i++) {
      const u = (i + 0.5) / n;
      const rho = Math.sqrt(1 - Math.pow(1 - u, 1 / 1.6));
      const th = a0 + i * GOLDEN;
      const j = r() * TAU, jr = Math.sqrt(r()) * spacing * 0.5;
      out[i * 3] = Math.cos(th) * rho * p.R + Math.cos(j) * jr;
      out[i * 3 + 1] = Math.sin(th) * rho * p.R + Math.sin(j) * jr;
      out[i * 3 + 2] = u;
    }
    return out;
  }

  /* Places whose pools touch are one pool, read as one and named after its largest place. Touching pools join
     nearest first, and stop joining once a pool would be wider than the largest single pool, so the cells of a dense
     grid make several pools rather than one. */
  function poolsOf(P, cap) {
    const shape = (members) => {
      const minutes = members.reduce((s, p) => s + p.minutes, 0);
      const x = members.reduce((s, p) => s + p.x * p.minutes, 0) / minutes, y = members.reduce((s, p) => s + p.y * p.minutes, 0) / minutes;
      const R = Math.max(...members.map((p) => Math.hypot(p.x - x, p.y - y) + p.R));
      return { members, minutes, x, y, R };
    };
    const group = P.map((p) => shape([p]));
    const of = P.map((_, i) => i);
    const pairs = [];
    for (let i = 0; i < P.length; i++) for (let j = i + 1; j < P.length; j++) {
      const d = Math.hypot(P[i].x - P[j].x, P[i].y - P[j].y);
      if (d < (P[i].R + P[j].R) * 0.9 + 1.5) pairs.push([d, i, j]);
    }
    pairs.sort((a, b) => a[0] - b[0]);
    for (const [, i, j] of pairs) {
      const gi = of[i], gj = of[j];
      if (gi === gj) continue;
      const merged = shape(group[gi].members.concat(group[gj].members));
      if (merged.R > Math.max(cap, group[gi].R, group[gj].R)) continue;
      group[gi] = merged; group[gj] = null;
      for (const p of merged.members) of[p.ix] = gi;
    }
    const pools = group.filter(Boolean).map((g) => {
      const lead = g.members.reduce((a, p) => (p.minutes > a.minutes ? p : a), g.members[0]);
      return { ...g, lead, label: lead.label };
    });
    pools.sort((a, b) => a.x - b.x || b.y - a.y);
    pools.forEach((pool, k) => { pool.k = k; for (const p of pool.members) p.pool = k; });
    return pools;
  }

  /* The river: a band of fine dots on an offset grid, largest down its middle and fading to its banks. */
  function halftone(b, water) {
    const path = new Path2D();
    if (!water.length) return path;
    const best = new Map();
    const hw = WATER_HW, s = WATER_STEP, rowH = s * 0.866;
    for (const line of water) {
      for (let i = 1; i < line.length; i++) {
        const [ax, ay] = line[i - 1], [bx, by] = line[i];
        const y0 = Math.max(b.top, Math.min(ay, by) - hw), y1 = Math.min(b.bottom, Math.max(ay, by) + hw);
        const x0 = Math.max(b.left, Math.min(ax, bx) - hw), x1 = Math.min(b.right, Math.max(ax, bx) + hw);
        if (x0 > x1 || y0 > y1) continue;
        for (let r = Math.ceil(y0 / rowH); r * rowH <= y1; r++) {
          const off = r % 2 ? s / 2 : 0;
          for (let c = Math.ceil((x0 - off) / s); c * s + off <= x1; c++) {
            const d = distSeg(c * s + off, r * rowH, ax, ay, bx, by);
            if (d >= hw) continue;
            const key = r * 100000 + c;
            if (!best.has(key) || best.get(key) > d) best.set(key, d);
          }
        }
      }
    }
    for (const [key, d] of best) {
      const r = Math.round(key / 100000), c = key - r * 100000;
      const rad = WATER_R * Math.sqrt(1 - (d / hw) ** 2);
      if (rad < 0.18) continue;
      const x = c * s + (r % 2 ? s / 2 : 0), y = r * rowH;
      path.moveTo(x + rad, y); path.arc(x, y, rad, 0, TAU);
    }
    return path;
  }

  /* The faint dotted ground, leaving the river's band clear. */
  function ground(b, water) {
    const path = new Path2D();
    const gx0 = b.left + ((b.right - b.left) % GRID) / 2, gy0 = b.top + ((b.bottom - b.top) % GRID) / 2;
    for (let y = gy0; y <= b.bottom + 0.1; y += GRID) {
      for (let x = gx0; x <= b.right + 0.1; x += GRID) {
        if (nearWater(x, y, water, WATER_HW + 2)) continue;
        path.moveTo(x + GRID_R, y); path.arc(x, y, GRID_R, 0, TAU);
      }
    }
    return path;
  }
  function nearWater(x, y, water, within) {
    for (const line of water) for (let i = 1; i < line.length; i++) {
      const [ax, ay] = line[i - 1], [bx, by] = line[i];
      if (x < Math.min(ax, bx) - within || x > Math.max(ax, bx) + within || y < Math.min(ay, by) - within || y > Math.max(ay, by) + within) continue;
      if (distSeg(x, y, ax, ay, bx, by) < within) return true;
    }
    return false;
  }

  /* Each named pool's name goes on the side of it that is clear of the other pools, the other names, the river, the
     scale bar and the plot's edge, largest pool first. A name with no clear side is left to the readout. */
  function placeLabels() {
    const b = G.b;
    measure.font = `${LABEL_FONT}px ${tokens.mono}`;
    const h = 10, placed = [];
    const scaleBox = { x0: b.left + 4, y0: b.bottom - 24, x1: b.left + 22 + G.scale.px + 44, y1: b.bottom };
    const DIRS = [[1, 0, 0], [-1, 0, 0.6], [0, -1, 1.2], [0, 1, 1.4], [0.71, -0.71, 1.8], [-0.71, -0.71, 2], [0.71, 0.71, 2.2], [-0.71, 0.71, 2.4]];
    const order = G.pools.filter((p) => p.label).sort((a, b2) => b2.minutes - a.minutes);
    for (const pool of order) {
      const w = measure.measureText(pool.label).width;
      let best = null;
      for (const [dx, dy, pref] of DIRS) {
        const off = pool.R + 8;
        const ax = pool.x + dx * off, ay = pool.y + dy * off;
        const x0 = dx > 0.3 ? ax : dx < -0.3 ? ax - w : ax - w / 2;
        const y0 = dy > 0.3 ? ay : dy < -0.3 ? ay - h : ay - h / 2;
        const c = { x0, y0, x1: x0 + w, y1: y0 + h };
        let score = pref;
        if (c.x0 < b.left + 2 || c.x1 > b.right - 2 || c.y0 < b.top + 2 || c.y1 > b.bottom - 2) score += 1000;
        if (overlap(c, scaleBox, 2)) score += 200;
        /* the river's band through a name reads as struck through, so a side clear of the water wins */
        if (G.water.length && [0, 0.25, 0.5, 0.75, 1].some((f) => [c.y0, (c.y0 + c.y1) / 2, c.y1].some((y) => nearWater(c.x0 + f * w, y, G.water, WATER_HW + 1.5)))) score += 120;
        for (const q of placed) if (overlap(c, q, 4)) score += 1000;
        for (const other of G.pools) {
          if (other === pool) continue;
          const nx = kit.clamp(other.x, c.x0, c.x1), ny = kit.clamp(other.y, c.y0, c.y1);
          const d = Math.hypot(other.x - nx, other.y - ny);
          if (d < other.R + 3) score += 40 + (other.R + 3 - d) * 4;
        }
        if (!best || score < best.score) best = { ...c, score };
      }
      if (best && best.score < 600) { best.pool = pool.k; best.text = pool.label; placed.push(best); }
    }
    return placed;
  }

  /* ---------- drawing ---------- */
  function fillOf(p) {
    if (mode === 'none' || reduced()) return 1;
    if (p.born === undefined) return 0;
    return kit.ease.out(kit.clamp((clock - p.born) / p.fill, 0, 1));
  }

  function draw() {
    if (!width) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const T = tokens, b = box(width);
    const live = mode === 'live' && !reduced();
    const groundA = live ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    ctx.save();
    ctx.beginPath(); ctx.rect(b.left, b.top, b.right - b.left, b.bottom - b.top); ctx.clip();

    if (groundA > 0 && G) {
      ctx.globalAlpha = groundA;
      ctx.fillStyle = kit.rgba(T.rgb.borderStrong, T.rgb.borderStrong[3] * 0.85);
      ctx.fill(G.ground);
      ctx.fillStyle = kit.rgba(T.rgb.subtle, T.rgb.subtle[3] * 0.62);
      ctx.fill(G.halftone);
      ctx.globalAlpha = 1;
    } else if (groundA > 0) {
      ctx.globalAlpha = groundA;
      ctx.fillStyle = kit.rgba(T.rgb.borderStrong, T.rgb.borderStrong[3] * 0.85);
      ctx.fill(ground(b, []));
      ctx.globalAlpha = 1;
    }
    if (!G) { ctx.restore(); return; }

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

    /* the dots, each pool filling from its middle outward; the pool you rest on stays, the others dim */
    const col = T.rgb.s0;
    const lit = new Path2D(), faint = new Path2D();
    const r0 = G.dotR;
    for (const p of G.P) {
      const f = fillOf(p);
      if (f <= 0) continue;
      const path = hover !== null && p.pool !== hover ? faint : lit;
      const D = p.dots;
      for (let i = 0; i < p.n; i++) {
        const u = D[i * 3 + 2];
        if (u > f) break;
        const r = f >= 1 ? r0 : r0 * kit.clamp((f - u) / 0.06, 0.25, 1);
        const x = p.x + D[i * 3], y = p.y + D[i * 3 + 1];
        path.moveTo(x + r, y); path.arc(x, y, r, 0, TAU);
      }
    }
    ctx.fillStyle = kit.rgba(col, col[3] * kit.lerp(1, 0.34, dim.x));
    ctx.fill(faint);
    ctx.fillStyle = kit.rgba(col);
    ctx.fill(lit);

    /* the ring around the pool you rest on */
    if (hover !== null && G.pools[hover]) {
      const pool = G.pools[hover];
      ctx.save();
      ctx.lineWidth = 1;
      ctx.strokeStyle = kit.rgba(col, col[3] * 0.9);
      ctx.beginPath(); ctx.arc(pool.x, pool.y, pool.R + 2 + 3 * ring.x, 0, TAU); ctx.stroke();
      ctx.restore();
    }

    /* the names, on a halo of the page so the ground never runs through them */
    ctx.save();
    ctx.font = `${LABEL_FONT}px ${T.mono}`;
    ctx.textAlign = 'left';
    ctx.textBaseline = 'alphabetic';
    ctx.lineJoin = 'round';
    ctx.lineWidth = 3;
    ctx.strokeStyle = T.page;
    for (const l of G.labels) {
      const pool = G.pools[l.pool];
      const a = labelAlpha(pool) * (hover !== null && hover !== l.pool ? kit.lerp(1, 0.55, dim.x) : 1);
      if (a <= 0) continue;
      ctx.globalAlpha = a;
      ctx.strokeText(l.text, l.x0, l.y1 - 1.5);
      ctx.fillStyle = hover === l.pool ? T.ink : T.muted;
      ctx.fillText(l.text, l.x0, l.y1 - 1.5);
    }
    ctx.restore();
    ctx.restore();
  }
  function labelAlpha(pool) {
    if (mode === 'none' || reduced()) return 1;
    const p = pool.lead;
    if (p.born === undefined) return 0;
    return kit.seg(clock - p.born, p.fill * 0.6, p.fill * 0.6 + 0.3);
  }

  /* ---------- the readout ---------- */
  const DOT_SWATCH = '<svg class="vk-sw" width="8" height="12" viewBox="0 0 8 12" aria-hidden="true"><circle cx="4" cy="6.5" r="1.6" style="fill:var(--vk-s0)"/></svg>';
  function readout() {
    if (!parts.readout) return;
    if (!M.drawn.length) { kit.setReadout(parts.readout, []); return; }
    if (hover === null || !G || !G.pools[hover]) {
      /* the full line, or in a narrow column the same facts in fewer words */
      const all = M.period ? `${aboutTime(M.total)} ${M.period}` : aboutTime(M.total);
      const forms = [
        (M.period ? [{ when: capital(M.period) }] : []).concat([{ swatch: DOT_SWATCH, label: 'One dot', value: unitWords(M.unit) }, { label: 'In all', value: aboutTime(M.total) }]),
        [{ swatch: DOT_SWATCH, label: 'One dot', value: unitWords(M.unit) }, { label: '', value: all }],
        [{ swatch: DOT_SWATCH, label: '', value: unitWords(M.unit) }, { label: '', value: all }],
      ];
      kit.setReadout(parts.readout, forms.find(fits) || forms[forms.length - 1]);
      return;
    }
    const pool = G.pools[hover];
    kit.setReadout(parts.readout, [{ when: pool.label || UNNAMED }, { label: '', value: `${aboutTime(pool.minutes)}${M.period ? ` ${M.period}` : ''}` }]);
  }

  /* whether a readout's parts fit on its one line: their words at the readout's size, the swatches and the gaps */
  function fits(list) {
    const cs = getComputedStyle(parts.readout);
    measure.font = `${cs.fontSize} ${cs.fontFamily}`;
    let w = 0;
    for (const p of list) {
      const words = [p.when, p.label, p.value].filter(Boolean);
      w += words.reduce((s, t) => s + measure.measureText(t).width, 0) + (words.length - 1) * 6 + (p.swatch ? 14 : 0) + 16;
    }
    return w - 16 <= parts.readout.clientWidth;
  }

  function setHover(k, from) {
    if (k === hover) { hoverFrom = from; return; }
    const was = hover;
    hover = k; hoverFrom = from;
    if (k !== null && was === null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    if (k !== null && was !== null) { ring.x = reduced() ? 1 : 0.4; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while pools fill, names fade in, a ring grows or the others dim ---------- */
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
      if (clock - revealAt < 0.35 || reveal < Infinity) moving = true;
      if (G) for (const p of G.P) if (p.born === undefined || clock - p.born < p.fill + 0.35) { moving = true; break; }
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    if (!G) return;
    for (const p of G.P) if (p.born === undefined && (reveal === Infinity || p.x <= reveal)) p.born = clock;
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
    if (mode === 'none') { mode = 'live'; revealAt = clock; }
    reveal = x;
    markBorn();
    rowsWait(reveal < Infinity);
    loop.wake();
  }

  /* ---------- pointer and keys ---------- */
  /* inside a pool, the one whose middle is nearest for its size; outside them all, the nearest edge within HIT */
  function nearest(px, py) {
    if (!G) return null;
    let best = null, score = Infinity;
    for (const p of G.P) {
      const d = Math.hypot(px - p.x, py - p.y) - p.R;
      const s = d < 0 ? d / p.R : d;
      if (s < score) { score = s; best = p.pool; }
    }
    return score <= HIT ? best : null;
  }
  const pick = (e) => { const r = canvas.getBoundingClientRect(); return nearest(e.clientX - r.left, e.clientY - r.top); };
  const ready = () => !!G && (mode === 'none' || reveal === Infinity);
  const onMove = (e) => {
    if (!ready()) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    const k = pick(e);
    if (k === null && e.pointerType === 'touch') return;
    setHover(k, k === null ? null : e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!ready()) return;
    const k = pick(e);
    if (e.pointerType === 'touch' && (k === null || (hoverFrom === 'touch' && hover === k))) { setHover(null, null); return; }
    setHover(k, k === null ? null : e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  parts.plot.addEventListener('pointermove', onMove);
  parts.plot.addEventListener('pointerdown', onDown);
  parts.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(parts.plot, {
    count: () => (ready() ? G.pools.length : 0),
    get: () => hover,
    set: (k, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(k, 'key');
    },
    page: 3,
  });

  /* ---------- setup and the handle ---------- */
  function label() {
    kit.setLabel(parts.plot, ariaLabel(M, G ? G.pools : M.drawn.map((p) => ({ label: p.label, minutes: p.minutes }))));
  }
  function apply() {
    M = model(out);
    hover = null; hoverFrom = null;
    layout();
    label();
    readout();
    kit.foot(parts.foot, out, opts);
    if (mode === 'live') { rowsWait(reveal < Infinity); loop.wake(); }
    draw();
  }
  const stopWidth = kit.observeWidth(parts.plot, (w) => {
    width = w;
    if (hover !== null) { hover = null; hoverFrom = null; }
    layout();
    label();
    readout();
    draw();
    if (mode === 'live') loop.wake();
  });
  const stopTheme = kit.watchTheme(root, () => {
    const t = kit.readTokens(root);
    if (t.signature !== tokens.signature) { tokens = t; if (G) G.labels = placeLabels(); draw(); }
  });
  const stopDpr = kit.watchDpr(() => draw());
  const stopMotion = kit.watchReducedMotion(() => { rowsWait(mode === 'live' && reveal < Infinity); loop.wake(); });
  /* the names are measured in the mono, so they are placed again once the page's fonts arrive */
  const fonts = typeof document !== 'undefined' && document.fonts && document.fonts.addEventListener ? document.fonts : null;
  const onFonts = () => { if (G) { G.labels = placeLabels(); draw(); } };
  if (fonts) fonts.addEventListener('loadingdone', onFonts);
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
      if (fonts) fonts.removeEventListener('loadingdone', onFonts);
      parts.plot.removeEventListener('pointermove', onMove);
      parts.plot.removeEventListener('pointerdown', onDown);
      parts.plot.removeEventListener('pointerleave', onLeave);
      root.remove();
    },
  };
}

function distSeg(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay, l2 = dx * dx + dy * dy;
  const f = l2 ? kit.clamp(((px - ax) * dx + (py - ay) * dy) / l2, 0, 1) : 0;
  return Math.hypot(ax + dx * f - px, ay + dy * f - py);
}
function overlap(a, b, gap) {
  return a.x0 < b.x1 + gap && a.x1 > b.x0 - gap && a.y0 < b.y1 + gap && a.y1 > b.y0 - gap;
}
