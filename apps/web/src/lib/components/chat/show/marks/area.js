// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/area.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* area.js: the stacked area chart. A filled line for amounts that build up or flow over time, with up to three
   series stacked into one shape, drawn in the dot grammar of the kept tiles.

   mark "area". It keeps ShowChart's line rules (apps/web/src/lib/components/chat/show/) and fills under them:
   - The first column is the x axis and the next one to three columns are the series. Rows arrive as show.rs shapes
     them: numbers, or null where the record has no value. 2 to 199 rows.
   - The fill starts at zero, because the area under the line is the amount. Two or three series stack into one
     shape, the first at the bottom, so the top edge is the total.
   - An x axis whose values are all times is spaced by time and drawn in time order, so a missing week reads as a
     longer step. Anything else is spaced evenly, in the order the query returned it.
   - A layer is drawn where it and every layer under it have a value, because its place in the stack depends on
     them. A null breaks that layer and the ones above it. A value below zero is drawn as zero; show.rs refuses one
     before it gets here.
   - Each edge is a line made of dots. The first series is the primary, its dots nearly touching, over a soft wash
     of the primary. The second is ink at 60% over a halftone of that ink, and the third subtle ink, its dots spaced
     wider, over a finer halftone. A dot marks each row while the rows are at least 12px apart.
   - The readout names the series, or the point you rest on: its x, the total when the series stack, and each
     series. In a narrow column the series names leave the line and stay for screen readers.
   - Rest on the chart, or tab to it and use the arrow keys: a dotted rule marks the x and a ring grows on each
     series' dot there.
   The rest of the grammar is series.js's: the y axis is a rule of fine dots with an ink dot at its top and its foot,
   and the grid is dotted. In the frame the shape draws in as the frame's sweep passes.

   Contract: see the top of kit.js. Block: readout 26 + plot 200 + foot 26 = 252px. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'area', kinds: ['chart'], marks: ['area'] });

const HEIGHT = kit.PLOT.height;
const PAD = kit.PLOT.pad;
const LABEL_W = 52;          /* a short date label is about 44px of 9.5px mono; each one gets 52 */
const END_R = 1.75;          /* the ink dots at the top and the foot of the y axis */
const MAX_SERIES = 3;
const HEADS_FROM = 12;       /* the rows' own dots show while the rows are at least this far apart */
const GROW = 0.45, GROW_BOUNCE = 0.1;
const TAU = Math.PI * 2;

/* Each layer: its edge as dots of diameter `lw`, one every `gap` px along the path; the dot on each row; and the
   fill under the edge. `color` is a kit series token. */
const LAYERS = Object.freeze([
  Object.freeze({ color: 's0', lw: 2, gap: 3, head: 2.5, hollow: false, fill: Object.freeze({ kind: 'wash', alpha: 0.15 }) }),
  Object.freeze({ color: 's1', lw: 1.7, gap: 3.2, head: 2.2, hollow: false, fill: Object.freeze({ kind: 'screen', color: 's1', r: 0.55, pitch: 2.5 }) }),
  Object.freeze({ color: 's2', lw: 1.5, gap: 4, head: 2.2, hollow: true, fill: Object.freeze({ kind: 'screen', color: 's2', r: 0.5, pitch: 3 }) }),
]);

const CSS = '.vk-area .vk-readout.vk-tight{gap:0 12px}';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);

function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = Array.isArray(out && out.rows) ? out.rows : [];
  const xKey = columns[0];
  const series = columns.slice(1, 1 + MAX_SERIES);
  const xs = rows.map((r) => r[xKey]);
  const dates = kit.isDateColumn(xs) || kit.isTimeColumn(xs);
  let times = null;
  if (dates) {
    const ts = xs.map((v) => { const t = kit.parseTime(v); return t ? t.getTime() : undefined; });
    if (ts.every((t) => t !== undefined)) times = ts;
  }
  const tLo = times ? Math.min(...times) : 0;
  const tHi = times ? Math.max(...times) : 0;
  const order = times ? rows.map((_, i) => i).sort((a, b) => times[a] - times[b]) : rows.map((_, i) => i);
  /* each row's running tops, layer by layer, up to the first series with no value */
  const tops = rows.map((r) => {
    const t = [];
    let acc = 0;
    for (const s of series) {
      const v = r[s];
      if (!isNum(v)) break;
      acc += Math.max(0, v);
      t.push(acc);
    }
    return t;
  });
  let hi = 0;
  for (const t of tops) if (t.length) hi = Math.max(hi, t[t.length - 1]);
  let ticks = kit.niceTicks(0, hi);
  if (ticks.length < 2) ticks = [0, hi || 1];
  return { columns, rows, xKey, series, xs, dates, times, tLo, tHi, order, tops, stacked: series.length > 1, ticks, yMin: ticks[0], yMax: ticks[ticks.length - 1] };
}

const listText = (names) => (names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}` : names[0] || '');

function ariaLabel(M) {
  let label = `${M.stacked ? 'Stacked area chart' : 'Area chart'} of ${listText(M.series.map(kit.columnLabel))} by ${kit.columnLabel(M.xKey || 'row')}`;
  if (M.stacked) label += ', from the bottom in that order';
  if (M.rows.length > 1) {
    const first = kit.formatX(M.xs[M.order[0]], M.dates), last = kit.formatX(M.xs[M.order[M.order.length - 1]], M.dates);
    if (first && last) label += `, from ${first} to ${last}`;
  }
  return `${label}.`;
}

/* The readout's mark for layer s: its fill with its edge of dots along the top. `sr` adds the series' name for
   screen readers when the line has no room to show it. */
function layerSwatch(s, sr) {
  const st = LAYERS[s] || LAYERS[0];
  const c = `var(--vk-${st.color})`;
  let fill;
  if (st.fill.kind === 'wash') fill = `<rect x="0" y="4" width="16" height="6" style="fill:color-mix(in srgb, ${c} ${Math.round(st.fill.alpha * 100)}%, transparent)"/>`;
  else {
    const step = st.fill.pitch, dots = [];
    for (let y = 6; y < 10; y += step) for (let x = 1.25; x < 16; x += step) dots.push(`<circle cx="${x}" cy="${y}" r="${st.fill.r}"/>`);
    fill = `<g style="fill:${c}">${dots.join('')}</g>`;
  }
  const edge = [];
  for (let x = 1.5; x < 16; x += st.gap) edge.push(`<circle cx="${x.toFixed(2)}" cy="3.5" r="${st.lw / 2}"/>`);
  return `<svg class="vk-sw" width="16" height="10" viewBox="0 0 16 10" aria-hidden="true">${fill}<g style="fill:${c}">${edge.join('')}</g></svg>${sr ? `<span class="vk-sr">${kit.esc(sr)}</span>` : ''}`;
}

/* Writes the first of `levels` that fits the readout's line; the last one is written whatever its width. The line's
   figures shrink before it overflows, so a level that does not fit shows as a name cut short. */
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
  kit.ensureCss('area', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-area');
  el.appendChild(root);
  const blk = kit.block(root, { plotHeight: HEIGHT, readout: true, foot: opts.foot !== false });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  blk.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 };
  /* The reveal: 'none' draws everything at once; 'live' raises each row as the frame's sweep passes it. */
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  let screens = new Map(), screensKey = '';
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry, as ShowChart lays out a line ---------- */
  const B = () => box(width);
  const Y = (v) => {
    const b = B(), plotH = b.bottom - b.top;
    return b.top + plotH - ((v - M.yMin) / (M.yMax - M.yMin || 1)) * plotH;
  };
  function xAt(i) {
    const b = B(), plotW = b.right - b.left, n = M.rows.length;
    if (M.times) return b.left + (M.tHi === M.tLo ? plotW / 2 : ((M.times[i] - M.tLo) / (M.tHi - M.tLo)) * plotW);
    return b.left + (n === 1 ? plotW / 2 : (i / (n - 1)) * plotW);
  }

  function growth(i) {
    if (mode === 'none') return 1;
    if (born[i] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[i], GROW, GROW_BOUNCE));
  }

  /* A halftone as a canvas pattern, one dot per tile in device pixels, so it stays crisp at any density. */
  function screen(ctx, f, k) {
    const key = `${k}|${tokens.signature}`;
    if (key !== screensKey) { screens = new Map(); screensKey = key; }
    const id = `${f.color}|${f.r}|${f.pitch}`;
    let s = screens.get(id);
    if (!s) {
      const pd = Math.max(2, Math.round(f.pitch * k));
      const c = document.createElement('canvas');
      c.width = pd; c.height = pd;
      const g = c.getContext('2d');
      g.fillStyle = kit.rgba(tokens.rgb[f.color]);
      g.beginPath(); g.arc(pd / 2, pd / 2, Math.max(0.5, f.r * k), 0, TAU); g.fill();
      s = { pattern: ctx.createPattern(c, 'repeat'), pd };
      screens.set(id, s);
    }
    return s;
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (ctx, x, y, r) => { ctx.moveTo(x + r, y); ctx.arc(x, y, r, 0, TAU); };

  /* The runs of rows, in drawing order, where layer s and every layer under it have a value. Each point is
     [row, x, edge y, base y]. */
  function runs(s) {
    const b = B(), floor = b.bottom;
    const out = [];
    let run = null;
    for (const i of M.order) {
      const t = M.tops[i];
      if (t.length <= s) { run = null; continue; }
      const g = growth(i);
      const x = xAt(i);
      const y = floor + (Y(t[s]) - floor) * g;
      const y0 = s ? floor + (Y(t[s - 1]) - floor) * g : floor;
      if (!run) out.push(run = []);
      run.push([i, x, y, y0]);
    }
    return out;
  }

  function draw() {
    if (!width || !M.rows.length) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const k = canvas.width / width;
    const T = tokens, b = B();
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const n = M.rows.length, S = M.series.length;
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

    /* x labels, as series.js places a line's: by index, or by time with any that would touch left out */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.fillStyle = T.muted;
    ctx.textBaseline = 'alphabetic';
    const labelled = kit.labelIndices(n, Math.max(2, Math.floor((R - L) / LABEL_W))).map((j) => M.order[j]);
    const room = Math.max(4, Math.floor(LABEL_W / 6) - 1);
    const fit = (text) => (text.length > room ? `${text.slice(0, room - 1)}…` : text);
    const placed = labelled.map((i, j) => {
      const x = xAt(i);
      const align = j === labelled.length - 1 && labelled.length > 1 ? 'right' : j === 0 ? 'left' : 'center';
      const text = M.dates ? kit.formatX(M.xs[i], true) : fit(kit.formatX(M.xs[i], false));
      const w = ctx.measureText(text).width;
      const x0 = align === 'left' ? x : align === 'right' ? x - w : x - w / 2;
      return { x, align, text, x0, x1: x0 + w };
    });
    const kept = [];
    placed.forEach((p, j) => {
      if (j === placed.length - 1) while (kept.length > 1 && p.x0 < kept[kept.length - 1].x1 + 6) kept.pop();
      if (!kept.length || p.x0 >= kept[kept.length - 1].x1 + 6) kept.push(p);
    });
    for (const p of kept) {
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - p.x, 0, 24) : 1);
      if (a <= 0) continue;
      ctx.globalAlpha = a;
      ctx.textAlign = p.align;
      ctx.fillText(p.text, p.x, HEIGHT - 6);
    }
    ctx.restore();

    /* the y axis: fine dots from the foot to the top with an ink dot at each end */
    dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); disc(ctx, L, top, END_R); disc(ctx, L, floor, END_R); ctx.fill();
    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    for (const t of M.ticks) ctx.fillText(kit.formatNumber(t), L - 6, Y(t));
    ctx.restore();

    const hv = hover;
    if (hv !== null) dotted(ctx, xAt(hv), top, xAt(hv), floor, 4, 1, T.borderStrong);

    const layerRuns = [];
    for (let s = 0; s < S; s++) layerRuns.push(runs(s));

    /* the fills, bottom to top, each laid on the page so the grid stops at the shape's edge */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    for (let s = 0; s < S; s++) {
      const st = LAYERS[s], f = st.fill;
      for (const run of layerRuns[s]) {
        if (run.length < 2) continue;
        const path = new Path2D();
        run.forEach(([, x, y], j) => (j ? path.lineTo(x, y) : path.moveTo(x, y)));
        for (let j = run.length - 1; j >= 0; j--) path.lineTo(run[j][1], run[j][3]);
        path.closePath();
        ctx.fillStyle = T.page;
        ctx.fill(path);
        if (f.kind === 'wash') {
          const col = T.rgb[st.color];
          if (M.stacked) ctx.fillStyle = kit.rgba(col, col[3] * f.alpha);
          else {
            /* one series: the wash fades toward the floor, so the edge carries the reading */
            const grad = ctx.createLinearGradient(0, top, 0, floor);
            grad.addColorStop(0, kit.rgba(col, col[3] * f.alpha * 1.5));
            grad.addColorStop(1, kit.rgba(col, col[3] * f.alpha * 0.25));
            ctx.fillStyle = grad;
          }
          ctx.fill(path);
        } else {
          const sc = screen(ctx, f, k);
          sc.pattern.setTransform(new DOMMatrix([1 / k, 0, 0, 1 / k, Math.round(L * k) / k, Math.round(floor * k) / k]));
          ctx.fillStyle = sc.pattern;
          ctx.fill(path);
        }
      }
    }

    /* the edges, top layer first, so the primary's dots lie on top where two edges meet */
    for (let s = S - 1; s >= 0; s--) {
      const st = LAYERS[s], col = T.rgb[st.color];
      ctx.strokeStyle = kit.rgba(col);
      ctx.lineWidth = st.lw; ctx.lineCap = 'round'; ctx.lineJoin = 'round';
      ctx.setLineDash([0, st.gap]);
      ctx.beginPath();
      for (const run of layerRuns[s]) run.forEach(([, x, y], j) => (j ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
      ctx.stroke();
      ctx.setLineDash([]);
    }
    ctx.restore();

    /* a dot on each row, and a ring on the row you rest on */
    const plotW = R - L;
    const dense = n > 1 && plotW / (n - 1) < HEADS_FROM;
    for (let s = S - 1; s >= 0; s--) {
      const st = LAYERS[s], col = T.rgb[st.color];
      const heads = new Path2D();
      let ringAt = null;
      for (const run of layerRuns[s]) {
        for (const [i, x, y] of run) {
          if (x > clipX) continue;
          if (dense && i !== hv) continue;
          const g = growth(i);
          const r = st.head * kit.clamp(g * 3, 0, 1) + (i === hv ? 0.5 : 0);
          if (r <= 0) continue;
          heads.moveTo(x + r, y); heads.arc(x, y, r, 0, TAU);
          if (i === hv) ringAt = [x, y];
        }
      }
      if (st.hollow) {
        ctx.fillStyle = T.page; ctx.fill(heads);
        ctx.strokeStyle = kit.rgba(col); ctx.lineWidth = 1; ctx.stroke(heads);
      } else { ctx.fillStyle = kit.rgba(col); ctx.fill(heads); }
      if (ringAt) {
        ctx.save();
        ctx.lineWidth = 1;
        ctx.strokeStyle = kit.rgba(col, Math.min(1, col[3] * 1.1) * 0.9);
        const rr = st.head + 1.5 + 3.5 * ring.x;
        ctx.beginPath(); ctx.arc(ringAt[0], ringAt[1], rr, 0, TAU); ctx.stroke();
        ctx.restore();
      }
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    const node = blk.readout;
    if (!node || !M.series.length) return;
    const names = M.series.map(kit.columnLabel);
    if (hover === null) {
      fitReadout(node, [M.stacked ? names.map((nm, s) => ({ swatch: layerSwatch(s), label: nm })) : [{ label: names[0] }]]);
      return;
    }
    const row = M.rows[hover];
    const when = { when: kit.formatX(M.xs[hover], M.dates) };
    const vals = M.series.map((s) => (isNum(row[s]) ? kit.formatNumber(row[s]) : '-'));
    if (!M.stacked) { fitReadout(node, [[when, { value: vals[0] }]]); return; }
    const t = M.tops[hover];
    const tv = t.length === M.series.length ? kit.formatNumber(t[t.length - 1]) : '-';
    fitReadout(node, [
      [when, { label: 'Total', value: tv }, ...vals.map((v, s) => ({ swatch: layerSwatch(s), label: names[s], value: v }))],
      [when, { label: 'Total', value: tv }, ...vals.map((v, s) => ({ swatch: layerSwatch(s, names[s]), value: v }))],
      [when, { swatch: '<span class="vk-sr">Total</span>', value: tv }, ...vals.map((v, s) => ({ swatch: layerSwatch(s, names[s]), value: v }))],
    ]);
  }

  function setHover(i, from) {
    if (i === hover) { hoverFrom = from; return; }
    hover = i; hoverFrom = from;
    if (i !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows or rows rise ---------- */
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
      for (let i = 0; i < born.length; i++) if (born[i] !== undefined && clock - born[i] < 1.2) { moving = true; break; }
      if (reveal < Infinity) moving = true;
    }
    draw();
    return moving;
  }

  /* ---------- the reveal the frame drives ---------- */
  function markBorn() {
    for (let i = 0; i < M.rows.length; i++) {
      if (born[i] === undefined && (reveal === Infinity || xAt(i) <= reveal)) born[i] = clock;
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
    for (let i = 0; i < M.rows.length; i++) { const d = Math.abs(xAt(i) - px); if (d < dist) { best = i; dist = d; } }
    return best;
  }
  /* A mouse reads the row under it and lets go when it leaves. A finger lifts as soon as it taps, so a tap keeps its
     row until the same row is tapped again or the chart loses focus. A drag across the chart scrubs. */
  const pick = (e) => nearest(e.clientX - canvas.getBoundingClientRect().left);
  const onMove = (e) => {
    if (!width || !M.rows.length) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.rows.length) return;
    const i = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === i) { setHover(null, null); return; }
    setHover(i, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  blk.plot.addEventListener('pointermove', onMove);
  blk.plot.addEventListener('pointerdown', onDown);
  blk.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(blk.plot, {
    count: () => M.rows.length,
    get: () => (hover === null ? null : M.order.indexOf(hover)),
    set: (j, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(j === null ? null : M.order[j], 'key');
    },
  });

  /* ---------- setup and the handle ---------- */
  function apply() {
    M = model(out);
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
