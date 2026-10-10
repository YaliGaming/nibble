(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.PetSprites = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  const MAX_SIDE = 32;
  const FRAME_NAMES = ['blink', 'walk1', 'walk2', 'happy', 'sad', 'alert', 'sleep'];
  const FALLBACK = { sleep: 'blink' };
  const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i;
  const isClear = (ch) => ch === '.' || ch === ' ';

  function validatePet(pet) {
    const errors = [];
    if (!pet || typeof pet !== 'object' || Array.isArray(pet)) return { ok: false, errors: ['pet must be a JSON object'] };
    if (typeof pet.name !== 'string' || !pet.name.trim()) errors.push('name is required');
    const palette = pet.palette && typeof pet.palette === 'object' ? pet.palette : null;
    if (!palette) errors.push('palette is required');
    else {
      for (const [ch, colour] of Object.entries(palette)) {
        if (ch.length !== 1 || isClear(ch)) errors.push(`palette key "${ch}" must be a single character other than "." or space`);
        if (typeof colour !== 'string' || !HEX.test(colour)) errors.push(`palette colour for "${ch}" must be #rgb or #rrggbb`);
      }
    }
    if (pet.outline !== undefined && pet.outline !== false && !(typeof pet.outline === 'string' && HEX.test(pet.outline))) {
      errors.push('outline must be false or a hex colour');
    }
    const base = pet.base;
    if (!Array.isArray(base) || !base.length || typeof base[0] !== 'string') {
      errors.push('base must be a non-empty array of strings');
      return { ok: false, errors };
    }
    const w = base[0].length;
    const h = base.length;
    if (w < 4 || h < 4 || w > MAX_SIDE || h > MAX_SIDE) errors.push(`base must be between 4x4 and ${MAX_SIDE}x${MAX_SIDE} (got ${w}x${h})`);
    const checkRow = (row, where) => {
      if (typeof row !== 'string' || row.length !== w) { errors.push(`${where} must be exactly ${w} characters`); return; }
      for (const ch of row) {
        if (!isClear(ch) && !(palette && typeof palette[ch] === 'string')) { errors.push(`${where}: "${ch}" is not in the palette`); return; }
      }
    };
    base.forEach((row, i) => checkRow(row, `base row ${i}`));
    if (pet.frames !== undefined && (typeof pet.frames !== 'object' || !pet.frames || Array.isArray(pet.frames))) {
      errors.push('frames must be an object');
    } else {
      for (const [name, patch] of Object.entries(pet.frames || {})) {
        if (!FRAME_NAMES.includes(name)) { errors.push(`unknown frame "${name}" (use ${FRAME_NAMES.join(', ')})`); continue; }
        if (!patch || typeof patch !== 'object' || Array.isArray(patch)) { errors.push(`frame ${name} must map row numbers to rows`); continue; }
        for (const [key, row] of Object.entries(patch)) {
          const i = Number(key);
          if (!Number.isInteger(i) || i < 0 || i >= h) errors.push(`frame ${name}: row "${key}" is out of range`);
          else checkRow(row, `frame ${name} row ${key}`);
        }
      }
    }
    return { ok: errors.length === 0, errors };
  }

  function frameRows(pet, name) {
    const frames = pet.frames || {};
    let n = name;
    while (n && n !== 'base' && !frames[n]) n = FALLBACK[n] || 'base';
    if (!n || n === 'base') return pet.base;
    const rows = pet.base.slice();
    for (const [key, row] of Object.entries(frames[n])) rows[Number(key)] = row;
    return rows;
  }

  function hexToRgb(hex) {
    let h = hex.slice(1);
    if (h.length === 3) h = h.split('').map((c) => c + c).join('');
    const n = parseInt(h, 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }

  const MARGIN = { x: 4, top: 7, bottom: 2 };
  const TUCK = 3;
  const OUTLINE = 'rgba(30, 23, 18, 0.62)';
  const frameCache = new WeakMap();

  function metrics(pet) {
    const w = pet.base[0].length;
    const h = pet.base.length;
    return { w, h, lw: w + MARGIN.x * 2, lh: h + MARGIN.top + MARGIN.bottom };
  }

  function frameCanvas(pet, name) {
    let cache = frameCache.get(pet);
    if (!cache) { cache = new Map(); frameCache.set(pet, cache); }
    if (cache.has(name)) return cache.get(name);
    const rows = frameRows(pet, name);
    const w = rows[0].length;
    const h = rows.length;
    const canvas = document.createElement('canvas');
    canvas.width = w + 2;
    canvas.height = h + 2;
    const ctx = canvas.getContext('2d');
    const solid = (x, y) => y >= 0 && y < h && x >= 0 && x < w && !isClear(rows[y][x]);
    if (pet.outline !== false) {
      ctx.fillStyle = typeof pet.outline === 'string' ? pet.outline : OUTLINE;
      for (let y = -1; y <= h; y++) {
        for (let x = -1; x <= w; x++) {
          if (!solid(x, y) && (solid(x - 1, y) || solid(x + 1, y) || solid(x, y - 1) || solid(x, y + 1))) ctx.fillRect(x + 1, y + 1, 1, 1);
        }
      }
    }
    for (let y = 0; y < h; y++) {
      for (let x = 0; x < w; x++) {
        const ch = rows[y][x];
        if (isClear(ch)) continue;
        ctx.fillStyle = pet.palette[ch];
        ctx.fillRect(x + 1, y + 1, 1, 1);
      }
    }
    cache.set(name, canvas);
    return canvas;
  }

  function blit(target, source) {
    const ctx = target.getContext('2d');
    ctx.clearRect(0, 0, target.width, target.height);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(source, 0, 0, target.width, target.height);
  }

  function drawStill(canvas, pet, frame, px) {
    const src = frameCanvas(pet, frame || 'base');
    const dpr = window.devicePixelRatio || 1;
    canvas.style.width = src.width * px + 'px';
    canvas.style.height = src.height * px + 'px';
    canvas.width = Math.round(src.width * px * dpr);
    canvas.height = Math.round(src.height * px * dpr);
    blit(canvas, src);
  }

  const STATE_FRAME = { idle: 'base', running: 'walk1', needs_input: 'alert', ready: 'happy', blocked: 'sad', sleeping: 'sleep' };

  const QUICK = 0;
  const GENTLE = 40;
  const MIN_FRAME = 12;

  class Animator {
    constructor(canvas, { shadow = null } = {}) {
      this.canvas = canvas;
      this.shadow = shadow;
      this.off = document.createElement('canvas');
      this.pet = null;
      this.state = 'idle';
      this.px = 6;
      this.since = performance.now();
      this.nextBlink = this.since + 2500;
      this.active = false;
      this.handle = 0;
      this.framed = false;
      this.painted = 0;
      this.drawn = '';
      this.moved = '';
      this.motion = window.matchMedia('(prefers-reduced-motion: reduce)');
      this.motion.addEventListener('change', () => this.kick());
    }

    setPet(pet) {
      if (pet === this.pet) return;
      this.pet = pet;
      this.resize();
    }

    setScale(px) {
      if (px === this.px) return;
      this.px = px;
      this.resize();
    }

    setState(state) {
      if (state === this.state) return;
      this.state = state;
      this.since = performance.now();
      this.kick();
    }

    resize() {
      if (!this.pet) return;
      const m = metrics(this.pet);
      const dpr = window.devicePixelRatio || 1;
      this.off.width = m.lw;
      this.off.height = m.lh;
      this.canvas.style.width = m.lw * this.px + 'px';
      this.canvas.style.height = m.lh * this.px + 'px';
      this.canvas.width = Math.round(m.lw * this.px * dpr);
      this.canvas.height = Math.round(m.lh * this.px * dpr);
      this.drawn = '';
      this.kick();
    }

    start() {
      if (this.active) return;
      this.active = true;
      this.kick();
    }

    stop() {
      this.active = false;
      this.cancel();
    }

    cancel() {
      if (this.framed) cancelAnimationFrame(this.handle); else clearTimeout(this.handle);
      this.handle = 0;
    }

    kick() {
      this.cancel();
      this.painted = 0;
      this.tick();
    }

    tick() {
      const now = performance.now();
      if (this.framed && now - this.painted < MIN_FRAME) { this.schedule(QUICK); return; }
      const wait = this.paint(now);
      this.painted = now;
      if (this.active && !this.motion.matches) this.schedule(wait);
    }

    schedule(wait) {
      this.framed = wait <= 0;
      this.handle = this.framed
        ? requestAnimationFrame(() => this.tick())
        : setTimeout(() => this.tick(), Math.min(Math.ceil(wait) + 1, 2000));
    }

    sample(now) {
      const t = now - this.since;
      const still = { phase: 0, x: 0, y: 0, lift: 0 };
      const arc = (progress) => Math.sin(Math.PI * progress);

      switch (this.state) {
        case 'running': {
          const step = 200;
          return { ...still, frame: Math.floor(t / step) % 2 ? 'walk2' : 'walk1', phase: Math.floor(t / (step * 2)) % 4, wait: step - (t % step) };
        }
        case 'needs_input': {
          const c = t % 2400;
          if (c < 400) { const lift = arc(c / 400); return { ...still, frame: 'alert', y: -2.6 * lift, lift, wait: QUICK }; }
          if (c < 680) { const lift = arc((c - 400) / 280); return { ...still, frame: 'alert', y: -1.2 * lift, lift: lift * 0.45, wait: QUICK }; }
          return { ...still, frame: 'alert', wait: 2400 - c };
        }
        case 'ready': {
          const phase = t < 2600 ? Math.floor(t / 180) : 1000 + Math.floor(t / 500);
          if (t < 1000) { const lift = Math.abs(arc(t / 500)); return { ...still, frame: 'happy', phase, y: -2.4 * lift, lift, wait: QUICK }; }
          return { ...still, frame: 'happy', phase, wait: t < 2600 ? 180 - (t % 180) : 500 - (t % 500) };
        }
        case 'blocked': {
          const c = t % 3800;
          if (c < 450) { const p = c / 450; return { ...still, frame: 'sad', x: 0.9 * Math.sin(p * Math.PI * 6) * (1 - p), wait: QUICK }; }
          return { ...still, frame: 'sad', wait: 3800 - c };
        }
        case 'sleeping':
          return { ...still, frame: 'sleep', phase: Math.floor(t / 420) % 8, wait: 420 - (t % 420) };
        default: {
          if (now > this.nextBlink + 160) this.nextBlink = now + 2600 + Math.random() * 3400;
          const blinking = now >= this.nextBlink;
          const untilBlink = blinking ? this.nextBlink + 160 - now : this.nextBlink - now;
          const c = t % 6000;
          const breathing = c < 800;
          return {
            ...still,
            frame: blinking ? 'blink' : 'base',
            y: breathing ? 0.4 * arc(c / 800) : 0,
            wait: Math.min(breathing ? GENTLE : 6000 - c, Math.max(untilBlink, 0)),
          };
        }
      }
    }

    paint(now = performance.now()) {
      if (!this.pet) return 2000;
      const s = this.motion.matches
        ? { frame: STATE_FRAME[this.state] || 'base', phase: -1, x: 0, y: 0, lift: 0, wait: 2000 }
        : this.sample(now);

      const drawing = [this.state, s.frame, s.phase].join('|');
      if (drawing !== this.drawn) {
        this.drawn = drawing;
        const m = metrics(this.pet);
        const ctx = this.off.getContext('2d');
        ctx.clearRect(0, 0, m.lw, m.lh);
        ctx.drawImage(frameCanvas(this.pet, s.frame), MARGIN.x - 1, MARGIN.top - 1);
        if (s.phase >= 0) this.effects(ctx, m, s.phase);
        blit(this.canvas, this.off);
      }

      const dpr = window.devicePixelRatio || 1;
      const snap = (spritePixels) => Math.round(spritePixels * this.px * dpr) / dpr;
      const place = `translate(${snap(s.x)}px, ${snap(s.y)}px)`;
      if (place !== this.moved) {
        this.moved = place;
        this.canvas.style.transform = place;
        if (this.shadow) {
          this.shadow.style.transform = `scale(${(1 - 0.3 * s.lift).toFixed(3)})`;
          this.shadow.style.opacity = (1 - 0.45 * s.lift).toFixed(3);
        }
      }
      return s.wait;
    }

    effects(ctx, m, phase) {
      const cx = Math.floor(m.lw / 2);
      const dot = (x, y, colour) => { ctx.fillStyle = colour; ctx.fillRect(x, y, 1, 1); };
      if (this.state === 'running') {
        for (let i = 0; i < 3; i++) dot(cx - 3 + i * 3 - 1, 3, i < phase ? '#E8824F' : 'rgba(232,130,79,0.28)');
      } else if (this.state === 'ready') {
        const spots = [[1, 8], [m.lw - 2, 6], [m.lw - 2, 14], [1, 16]];
        spots.forEach(([x, y], i) => {
          const on = phase < 1000 ? (phase + i) % 3 !== 0 : (phase + i * 3) % 8 === 0;
          if (!on) return;
          dot(x, y, '#FFD65A');
          if ((phase + i) % 2) { dot(x - 1, y, '#FFE9A3'); dot(x + 1, y, '#FFE9A3'); dot(x, y - 1, '#FFE9A3'); dot(x, y + 1, '#FFE9A3'); }
        });
      } else if (this.state === 'sleeping') {
        for (const offset of [0, 4]) {
          const k = ((phase + offset) % 8) / 8;
          const x = cx + 3 + Math.round(k * 4);
          const y = MARGIN.top - 3 - Math.round(k * 4);
          ctx.globalAlpha = 1 - k;
          ctx.fillStyle = '#8FA8D6';
          ctx.fillRect(x, y, 4, 1);
          ctx.fillRect(x + 2, y + 1, 1, 1);
          ctx.fillRect(x + 1, y + 2, 1, 1);
          ctx.fillRect(x, y + 3, 4, 1);
          ctx.globalAlpha = 1;
        }
      }
    }
  }

  return { validatePet, frameRows, hexToRgb, metrics, drawStill, Animator, FRAME_NAMES, MARGIN, TUCK, MAX_SIDE };
});
