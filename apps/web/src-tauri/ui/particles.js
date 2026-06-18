// Vanilla particle-dissolve — ported from apps/web/src/lib/components/LoginInput.svelte
// (its canvas engine, minus the Svelte reactivity + caret-follow). Renders text
// into an offscreen 2× buffer, samples opaque pixels into particles, then sweeps
// a cursor right-to-left dissolving them. Cosmetic only; calls `onDone` when the
// last particle fades. Exposed as `window.VirtuesParticles.dissolveText`.
(function () {
  const SCALE = 2; // supersample for crisp particles

  // dissolveText(canvas, { text, font, fontSize, color, width, height, onDone })
  function dissolveText(canvas, opts) {
    const ctx = canvas && canvas.getContext ? canvas.getContext("2d") : null;
    const text = (opts && opts.text) || "";
    const done = (opts && opts.onDone) || function () {};
    if (!ctx || !text) {
      done();
      return;
    }

    const W = opts.width || canvas.clientWidth || 320;
    const H = opts.height || canvas.clientHeight || 56;
    // CSS size at 1×, backing store at 2× → crisp on retina.
    canvas.style.width = W + "px";
    canvas.style.height = H + "px";
    canvas.width = W * SCALE;
    canvas.height = H * SCALE;

    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.font = (opts.fontSize || 26) * SCALE + "px " + (opts.font || "monospace");
    ctx.fillStyle = opts.color || "#14283D";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(text, canvas.width / 2, canvas.height / 2);

    const img = ctx.getImageData(0, 0, canvas.width, canvas.height);
    let particles = [];
    let maxX = 0;
    for (let y = 0; y < canvas.height; y += SCALE) {
      for (let x = 0; x < canvas.width; x += SCALE) {
        const i = (y * canvas.width + x) * 4;
        const a = img.data[i + 3];
        if (a > 0) {
          const dx = x / SCALE;
          const dy = y / SCALE;
          if (dx > maxX) maxX = dx;
          particles.push({
            x: dx,
            y: dy,
            originX: dx,
            r: 1,
            color: "rgba(" + img.data[i] + "," + img.data[i + 1] + "," + img.data[i + 2] + "," + a / 255 + ")",
            vx: Math.random() * -0.4 - 0.1,
            vy: Math.random() * 0.6 - 0.3,
            active: false,
          });
        }
      }
    }

    let sweepX = maxX + 5;
    function frame() {
      sweepX -= 3;
      particles.forEach(function (p) {
        if (!p.active && p.originX >= sweepX) p.active = true;
      });
      particles = particles
        .map(function (p) {
          if (p.active) {
            p.x += p.vx * 0.5;
            p.y += p.vy * 0.5;
            p.r -= 0.008;
          }
          return p.r > 0 ? p : null;
        })
        .filter(Boolean);

      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.save();
      ctx.scale(SCALE, SCALE); // draw in 1× coords on the 2× backing store
      particles.forEach(function (p) {
        ctx.fillStyle = p.color;
        if (p.active) {
          ctx.beginPath();
          ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
          ctx.fill();
        } else {
          ctx.fillRect(p.originX, p.y, 1, 1);
        }
      });
      ctx.restore();

      if (particles.length > 0) {
        requestAnimationFrame(frame);
      } else {
        ctx.clearRect(0, 0, canvas.width, canvas.height);
        done();
      }
    }
    frame();
  }

  window.VirtuesParticles = { dissolveText: dissolveText };
})();
