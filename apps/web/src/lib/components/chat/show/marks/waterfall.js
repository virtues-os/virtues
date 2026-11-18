// @ts-nocheck
// Ported unchanged from the dot labs chart kit (charts/waterfall.js in the design folder).
// Plain JavaScript drawn on a canvas; Show.svelte mounts it through ShowMark.svelte.
// Type checking is off because the module is untyped JS, not because of an error.

/* waterfall.js: the waterfall chart. How an amount moved from where it started to where it ended, one named step at
   a time, drawn in the dot grammar of the kept tiles.

   mark "waterfall". A new chart mark:
   - Two columns, by position: a label, then an amount (positive adds, negative takes away). Rows arrive as show.rs
     shapes them; 2 to 20 rows, in the query's order.
   - The running total starts at zero, so the first row is drawn from zero: it is the start, such as an opening
     balance or a first reading. Each later row floats from the running total before it, rising or falling. After the
     last row the chart draws the end itself, from zero to where the steps finish, labelled End. The query returns
     no end row.
   - No red and no green. The start and the end are the primary, solid. A step that adds is ink at 60%, solid. A
     step that takes away is the same ink as a hairline outline with a halftone inside, the amount that left. A dotted
     connector carries the running total from each bar to the next, so you see each step leave from where the last
     one landed.
   - Each bar carries its amount above it, signed for a step, while every amount fits its band. Labels sit under the
     bars on one line, on two lines when a word wraps, or on two staggered rows when a column is narrow, and are cut
     to fit only after that.
   - The readout names what the bars mean, or the bar you rest on: "Rent, −1450 to 3652", with the app's number
     format, which groups digits from five.
   - Rest on a bar, or tab to the chart and use the arrow keys: the other bars dim and a hairline ring grows around
     the bar.
   The rest of the grammar is series.js's: the y axis is a rule of fine dots with an ink dot at its top and its foot,
   and the grid is dotted. In the frame each bar grows from where it leaves as the frame's
   sweep passes it, and its connector follows.

   Contract: see the top of kit.js. Block: readout 26 + plot 200 + foot 26 = 252px. The plot keeps 34px under the
   floor for two rows of labels. */
import * as kit from './kit.js';

export const meta = Object.freeze({ name: 'waterfall', kinds: ['chart'], marks: ['waterfall'] });

const HEIGHT = kit.PLOT.height;
const PAD = Object.freeze({ top: kit.PLOT.pad.top, right: kit.PLOT.pad.right, bottom: 34, left: kit.PLOT.pad.left });
const END_R = 1.75;          /* the ink dots at the top and the foot of the y axis */
const MAX_ROWS = 20;
const HEADROOM = 0.08;       /* the share of the range kept above the highest bar for its amount */
const LINE_1 = 14, LINE_2 = 25.5;   /* label baselines under the floor */
const GROW = 0.45, GROW_BOUNCE = 0.1;
const SCREEN = Object.freeze({ color: 's1', r: 0.8, pitch: 3 });
const TAU = Math.PI * 2;
const MINUS = '−';

const CSS = '.vk-waterfall .vk-readout.vk-tight{gap:0 12px}';

export function blockHeight(input, options = {}) {
  return kit.ROWS.readout + HEIGHT + (options.foot === false ? 0 : kit.ROWS.foot);
}

export function box(width) {
  return kit.plotBox(width, HEIGHT, PAD);
}

const isNum = (v) => typeof v === 'number' && Number.isFinite(v);
const signed = (n) => (n > 0 ? `+${kit.formatNumber(n)}` : n < 0 ? `${MINUS}${kit.formatNumber(-n)}` : '0');
const plain = (n) => (n < 0 ? `${MINUS}${kit.formatNumber(-n)}` : kit.formatNumber(n));

/* Each bar: { label, amount, from, to, kind } with kind total, rise, fall or flat. */
function model(out) {
  const columns = Array.isArray(out && out.columns) ? out.columns : [];
  const rows = (Array.isArray(out && out.rows) ? out.rows : []).slice(0, MAX_ROWS);
  const labelKey = columns[0], amountKey = columns[1];
  const bars = [];
  let run = 0;
  rows.forEach((r, i) => {
    const a = isNum(r[amountKey]) ? r[amountKey] : null;
    const label = kit.formatCell(r[labelKey]) || '-';
    const from = run, to = run + (a === null ? 0 : a);
    const kind = i === 0 ? 'total' : a === null || a === 0 ? 'flat' : a > 0 ? 'rise' : 'fall';
    bars.push({ label, amount: a, from: i === 0 ? 0 : from, to, kind });
    run = to;
  });
  if (bars.length) bars.push({ label: 'End', amount: null, from: 0, to: run, kind: 'total', end: true });
  let lo = 0, hi = 0;
  for (const b of bars) { lo = Math.min(lo, b.from, b.to); hi = Math.max(hi, b.from, b.to); }
  let ticks = kit.niceTicks(lo, hi + (hi - lo || 1) * HEADROOM);
  if (ticks.length < 2) ticks = [lo, hi === lo ? lo + 1 : hi];
  return { columns, rows, labelKey, amountKey, bars, ticks, yMin: ticks[0], yMax: ticks[ticks.length - 1] };
}

function ariaLabel(M) {
  if (!M.bars.length) return 'Waterfall chart with no steps';
  const first = M.bars[0], end = M.bars[M.bars.length - 1];
  const steps = M.bars.slice(1, -1).map((b) => `${b.label} ${b.amount === null ? 'no amount' : signed(b.amount)}`);
  const what = M.amountKey ? ` of ${kit.columnLabel(M.amountKey)}` : '';
  return `Waterfall chart${what}, from ${first.label} at ${plain(first.to)} to the end at ${plain(end.to)}${steps.length ? `: ${steps.join(', ')}` : ''}.`;
}

/* The readout's mark for a kind of bar, drawn the way the bar draws it. */
function kindSwatch(kind) {
  let body;
  if (kind === 'total') body = '<rect x="0" y="0" width="8" height="10" rx="1" style="fill:var(--vk-s0)"/>';
  else if (kind === 'rise') body = '<rect x="0" y="0" width="8" height="10" rx="1" style="fill:var(--vk-s1)"/>';
  else {
    const dots = [];
    for (let y = 2.5; y < 9; y += 2.5) for (let x = 2.5; x < 7; x += 3) dots.push(`<circle cx="${x}" cy="${y}" r=".8"/>`);
    body = `<rect x=".5" y=".5" width="7" height="9" rx="1" style="fill:none;stroke:var(--vk-s1);stroke-width:1"/><g style="fill:var(--vk-s1)">${dots.join('')}</g>`;
  }
  return `<svg class="vk-sw" width="8" height="10" viewBox="0 0 8 10" aria-hidden="true">${body}</svg>`;
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

/* A label's words in at most two lines of `room` px, or null when a word is wider than the room or a third line
   would be needed. */
function wrap2(ctx, text, room) {
  const words = text.split(/\s+/).filter(Boolean);
  if (!words.length) return [''];
  const lines = [];
  let line = '';
  for (const w of words) {
    if (ctx.measureText(w).width > room) return null;
    const next = line ? `${line} ${w}` : w;
    if (ctx.measureText(next).width <= room) line = next;
    else { lines.push(line); line = w; }
  }
  lines.push(line);
  return lines.length <= 2 ? lines : null;
}

/* Cuts text to `room` px with an ellipsis. */
function cut(ctx, text, room) {
  if (ctx.measureText(text).width <= room) return text;
  let t = text;
  while (t.length > 1 && ctx.measureText(`${t}…`).width > room) t = t.slice(0, -1);
  return `${t.trimEnd()}…`;
}

export function mount(el, output, options = {}) {
  kit.ensureStyles();
  kit.ensureCss('waterfall', CSS);
  let opts = { ...options };
  const root = kit.h('div', 'vk vk-waterfall');
  el.appendChild(root);
  const blk = kit.block(root, {
    plotHeight: HEIGHT, readout: true, foot: opts.foot !== false,
    hint: 'Use the left and right arrow keys to read each step.',
  });
  const canvas = kit.h('canvas', null, { 'aria-hidden': 'true' });
  blk.plot.appendChild(canvas);

  let out = output || {};
  let M = model(out);
  let width = 0;
  let tokens = kit.readTokens(root);
  let hover = null, hoverFrom = null;
  const ring = { x: 1, v: 0 }, dim = { x: 0, v: 0 };
  /* The reveal: 'none' draws everything at once; 'live' grows each bar as the frame's sweep passes it. */
  let mode = Number.isFinite(opts.reveal) ? 'live' : 'none';
  let reveal = Number.isFinite(opts.reveal) ? opts.reveal : Infinity;
  let born = [];
  let clock = 0, revealAt = 0;
  let screenTile = null, screenKey = '';
  const reduced = () => (typeof opts.reduced === 'boolean' ? opts.reduced : kit.prefersReducedMotion());
  const loop = kit.loop(step);

  /* ---------- geometry ---------- */
  const B = () => box(width);
  const Y = (v) => {
    const b = B(), plotH = b.bottom - b.top;
    return b.top + plotH - ((v - M.yMin) / (M.yMax - M.yMin || 1)) * plotH;
  };
  const band = () => (B().right - B().left) / Math.max(1, M.bars.length);
  const xAt = (j) => B().left + band() * (j + 0.5);
  const barWidth = () => kit.clamp(band() * 0.56, 3, 40);

  function growth(j) {
    if (mode === 'none') return 1;
    if (born[j] === undefined) return 0;
    if (reduced()) return 1;
    return Math.max(0, kit.stepSpring(clock - born[j], GROW, GROW_BOUNCE));
  }

  /* The halftone inside a step that takes away, as a canvas pattern in device pixels. */
  function screen(ctx, k) {
    const key = `${k}|${tokens.signature}`;
    if (key !== screenKey || !screenTile) {
      const pd = Math.max(2, Math.round(SCREEN.pitch * k));
      const c = document.createElement('canvas');
      c.width = pd; c.height = pd;
      const g = c.getContext('2d');
      g.fillStyle = kit.rgba(tokens.rgb[SCREEN.color]);
      g.beginPath(); g.arc(pd / 2, pd / 2, Math.max(0.5, SCREEN.r * k), 0, TAU); g.fill();
      screenTile = { pattern: ctx.createPattern(c, 'repeat'), pd };
      screenKey = key;
    }
    return screenTile;
  }

  /* ---------- drawing ---------- */
  function dotted(ctx, x1, y1, x2, y2, gap, lw, color) {
    ctx.save();
    ctx.strokeStyle = color; ctx.lineWidth = lw; ctx.lineCap = 'round'; ctx.setLineDash([0, gap]);
    ctx.beginPath(); ctx.moveTo(x1, y1); ctx.lineTo(x2, y2); ctx.stroke();
    ctx.restore();
  }
  const disc = (ctx, x, y, r) => { ctx.moveTo(x + r, y); ctx.arc(x, y, r, 0, TAU); };

  /* Where each bar sits at this width, grown as far as the reveal has taken it. */
  function layout(k) {
    const snap = (v) => Math.round(v * k) / k;
    const bw = barWidth();
    return M.bars.map((bar, j) => {
      const g = growth(j);
      const cx = xAt(j);
      const x0 = snap(cx - bw / 2), x1 = Math.max(x0 + 1 / k, snap(cx + bw / 2));
      const end = bar.from + (bar.to - bar.from) * g;
      const ya = snap(Y(Math.max(bar.from, end))), yb = snap(Y(Math.min(bar.from, end)));
      return { bar, j, g, cx, x0, x1, ya, yb, yEnd: Math.round(Y(end)) };
    });
  }

  function draw() {
    if (!width || !M.bars.length) return;
    const ctx = kit.fitCanvas(canvas, width, HEIGHT);
    const k = canvas.width / width;
    const T = tokens, b = B();
    const L = b.left, R = b.right, top = b.top, floor = b.bottom;
    const live = mode === 'live';
    const clipX = live && reveal < Infinity ? reveal + 3 : Infinity;
    const labelA = live && !reduced() ? kit.seg(clock - revealAt, 0, 0.3) : 1;
    const bandW = band();
    const P = layout(k);

    /* the grid, zero and the floor arrive behind the sweep */
    ctx.save();
    if (clipX < Infinity) { ctx.beginPath(); ctx.rect(0, 0, clipX, HEIGHT); ctx.clip(); }
    for (const t of M.ticks) {
      const y = Math.round(Y(t));
      if (Math.abs(y - floor) < 1) continue;
      if (t === 0) { ctx.fillStyle = T.borderStrong; ctx.fillRect(L, y, R - L, 1); }
      else dotted(ctx, L, y + 0.5, R, y + 0.5, 5, 1.1, T.borderStrong);
    }
    ctx.fillStyle = T.borderStrong;
    ctx.fillRect(L, floor, R - L, 1);
    ctx.restore();

    /* the labels under the bars: one line, two lines, or two staggered rows, cut only after that */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.textBaseline = 'alphabetic';
    ctx.textAlign = 'center';
    const room = bandW - 6;
    const texts = M.bars.map((bar) => bar.label);
    let lines;
    if (texts.every((t) => ctx.measureText(t).width <= room)) lines = texts.map((t) => [[t, LINE_1]]);
    else {
      const wrapped = texts.map((t) => wrap2(ctx, t, room));
      if (wrapped.every(Boolean)) lines = wrapped.map((w) => w.map((t, r) => [t, r ? LINE_2 : LINE_1]));
      else lines = texts.map((t, j) => [[cut(ctx, t, bandW * 2 - 5), j % 2 ? LINE_2 : LINE_1]]);
    }
    P.forEach((p, j) => {
      const a = labelA * (clipX < Infinity ? kit.seg(clipX - p.cx, 0, 24) : 1);
      if (a <= 0) return;
      ctx.globalAlpha = a;
      ctx.fillStyle = hover === j ? T.ink : T.muted;
      for (const [t, dy] of lines[j]) ctx.fillText(t, p.cx, floor + dy);
    });
    ctx.restore();

    /* the y axis: fine dots from the foot to the top with an ink dot at each end */
    dotted(ctx, L, floor, L, top, 5.5, 1.3, T.muted);
    ctx.fillStyle = T.ink;
    ctx.beginPath(); disc(ctx, L, top, END_R); disc(ctx, L, floor, END_R); ctx.fill();
    ctx.save();
    ctx.globalAlpha = labelA;
    ctx.font = `9.5px ${T.mono}`; ctx.fillStyle = T.muted; ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    for (const t of M.ticks) ctx.fillText(plain(t), L - 6, Y(t));
    ctx.restore();

    const hv = hover;
    const dimA = hv !== null ? kit.lerp(1, 0.45, dim.x) : 1;
    const alphaOf = (j) => (hv !== null && hv !== j ? dimA : 1);

    /* the connectors: the running total, dotted from where a bar lands to where the next one leaves */
    for (let j = 0; j + 1 < P.length; j++) {
      const p = P[j], q = P[j + 1];
      if (p.g <= 0 || q.g <= 0) continue;
      const y = p.yEnd + 0.5;
      ctx.globalAlpha = kit.clamp(q.g * 2, 0, 1) * Math.min(alphaOf(j), alphaOf(j + 1));
      dotted(ctx, p.x1 + 2, y, q.x0 - 2, y, 3, 1, T.muted);
    }
    ctx.globalAlpha = 1;

    /* the bars */
    for (const p of P) {
      if (p.g <= 0) continue;
      const { bar, x0, x1, ya, yb } = p;
      ctx.globalAlpha = alphaOf(p.j);
      const h = yb - ya;
      if (bar.kind === 'flat' || h < 1) {
        ctx.fillStyle = T.s1;
        ctx.fillRect(x0, p.yEnd, x1 - x0, 1);
        continue;
      }
      if (bar.kind === 'total' || bar.kind === 'rise') {
        ctx.fillStyle = bar.kind === 'total' ? T.s0 : T.s1;
        ctx.fillRect(x0, ya, x1 - x0, h);
      } else {
        ctx.fillStyle = T.page;
        ctx.fillRect(x0, ya, x1 - x0, h);
        const s = screen(ctx, k);
        const tx = Math.round(p.cx * k - s.pd / 2), ty = Math.round(ya * k);
        s.pattern.setTransform(new DOMMatrix([1 / k, 0, 0, 1 / k, tx / k, ty / k]));
        ctx.fillStyle = s.pattern;
        ctx.fillRect(x0 + 1, ya + 1, Math.max(0, x1 - x0 - 2), Math.max(0, h - 2));
        ctx.strokeStyle = T.s1;
        ctx.lineWidth = 1;
        ctx.strokeRect(x0 + 0.5, ya + 0.5, Math.max(0, x1 - x0 - 1), Math.max(0, h - 1));
      }
    }
    ctx.globalAlpha = 1;

    /* each bar's amount above it, while every amount fits its band */
    ctx.save();
    ctx.font = `9.5px ${T.mono}`;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'alphabetic';
    const amounts = M.bars.map((bar) => (bar.kind === 'total' ? plain(bar.to) : bar.amount === null ? '' : signed(bar.amount)));
    if (amounts.every((t) => ctx.measureText(t).width <= bandW - 4)) {
      P.forEach((p, j) => {
        if (p.g <= 0 || !amounts[j]) return;
        ctx.globalAlpha = kit.clamp((p.g - 0.6) / 0.4, 0, 1) * alphaOf(j);
        ctx.fillStyle = hv === j ? T.ink : T.muted;
        ctx.fillText(amounts[j], p.cx, Math.min(p.ya, Math.round(Y(p.bar.to))) - 7);
      });
    }
    ctx.restore();

    /* the ring: the bar's outline lifted off it, growing as you rest */
    if (hv !== null && P[hv] && P[hv].g > 0) {
      const p = P[hv];
      const o = 1 + 2.5 * ring.x;
      const ya = Math.min(p.ya, Math.round(Y(p.bar.to))), yb = Math.max(p.yb, ya + 1);
      const col = p.bar.kind === 'total' ? T.rgb.s0 : T.rgb.ink;
      ctx.save();
      ctx.strokeStyle = kit.rgba(col, col[3] * 0.6);
      ctx.lineWidth = 1;
      ctx.beginPath();
      const w = p.x1 - p.x0 + 2 * o, hh = yb - ya + 2 * o;
      if (typeof ctx.roundRect === 'function') ctx.roundRect(p.x0 - o, ya - o, w, hh, Math.min(3, w / 2));
      else ctx.rect(p.x0 - o, ya - o, w, hh);
      ctx.stroke();
      ctx.restore();
    }
  }

  /* ---------- the readout ---------- */
  function readout() {
    const node = blk.readout;
    if (!node || !M.bars.length) return;
    if (hover === null) {
      fitReadout(node, [[
        { swatch: kindSwatch('total'), label: 'Start and end' },
        { swatch: kindSwatch('rise'), label: 'Adds' },
        { swatch: kindSwatch('fall'), label: 'Takes away' },
      ]]);
      return;
    }
    const bar = M.bars[hover];
    if (bar.kind === 'total') { fitReadout(node, [[{ when: bar.label }, { swatch: kindSwatch('total'), value: plain(bar.to) }]]); return; }
    const amount = bar.amount === null ? '-' : signed(bar.amount);
    fitReadout(node, [[{ when: bar.label }, { swatch: kindSwatch(bar.kind === 'fall' ? 'fall' : 'rise'), value: amount }, { label: 'to', value: plain(bar.to) }]]);
  }

  function setHover(j, from) {
    if (j === hover) { hoverFrom = from; return; }
    hover = j; hoverFrom = from;
    if (j !== null) { ring.x = reduced() ? 1 : 0; ring.v = 0; }
    readout();
    loop.wake();
  }

  /* ---------- the loop: runs only while a ring grows, the others dim, or bars grow ---------- */
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
    for (let j = 0; j < M.bars.length; j++) {
      if (born[j] === undefined && (reveal === Infinity || xAt(j) <= reveal)) born[j] = clock;
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
  const nearest = (px) => kit.clamp(Math.floor((px - B().left) / band()), 0, M.bars.length - 1);
  /* A mouse reads the bar under it and lets go when it leaves. A finger lifts as soon as it taps, so a tap keeps its
     bar until the same bar is tapped again or the chart loses focus. A drag across the chart scrubs. */
  const pick = (e) => nearest(e.clientX - canvas.getBoundingClientRect().left);
  const onMove = (e) => {
    if (!width || !M.bars.length) return;
    if (e.pointerType === 'touch' && !e.buttons) return;
    setHover(pick(e), e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onDown = (e) => {
    if (!width || !M.bars.length) return;
    const j = pick(e);
    if (e.pointerType === 'touch' && hoverFrom === 'touch' && hover === j) { setHover(null, null); return; }
    setHover(j, e.pointerType === 'touch' ? 'touch' : 'pointer');
  };
  const onLeave = () => { if (hoverFrom === 'pointer') setHover(null, null); };
  blk.plot.addEventListener('pointermove', onMove);
  blk.plot.addEventListener('pointerdown', onDown);
  blk.plot.addEventListener('pointerleave', onLeave);
  const stopKeys = kit.keyNav(blk.plot, {
    count: () => M.bars.length,
    get: () => hover,
    set: (j, why) => {
      if (why === 'blur') { if (hoverFrom === 'key' || hoverFrom === 'touch') setHover(null, null); return; }
      setHover(j, 'key');
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
