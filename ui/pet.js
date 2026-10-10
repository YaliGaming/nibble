'use strict';
const api = window.petAPI;
const $ = (id) => document.getElementById(id);

const stage = $('stage');
const hit = $('hit');
const animator = new PetSprites.Animator($('pet'), { shadow: $('shadow') });
let state = null;
let t = PetI18n.translator({});
let bubbleSid = null;
let askSid = null;

let ignoring = true;
function setIgnoring(ignore) {
  if (ignore === ignoring) return;
  ignoring = ignore;
  api.setIgnore(ignore);
}

function pointerAt(x, y) {
  const el = document.elementFromPoint(x, y);
  setIgnoring(!(el && el.closest('[data-hit]')));
  const r = $('hover-area').getBoundingClientRect();
  document.body.classList.toggle('hover', x >= r.left - 6 && x <= r.right + 6 && y >= r.top - 6 && y <= r.bottom);
}

api.onPointer((point) => {
  if (press) return;
  if (point) { pointerAt(point.x, point.y); return; }
  ignoring = true;
  document.body.classList.remove('hover');
});
window.addEventListener('mousemove', (e) => pointerAt(e.clientX, e.clientY));
document.documentElement.addEventListener('mouseleave', () => {
  if (press) return;
  setIgnoring(true);
  document.body.classList.remove('hover');
});

let press = null;
hit.addEventListener('pointerdown', (e) => {
  if (e.button !== 0) return;
  hit.setPointerCapture(e.pointerId);
  press = { x: e.screenX, y: e.screenY, dragging: false };
});
hit.addEventListener('pointermove', (e) => {
  if (!press || press.dragging) return;
  if (Math.hypot(e.screenX - press.x, e.screenY - press.y) > 4) {
    press.dragging = true;
    stage.classList.add('dragging');
    api.dragStart();
    followPointer();
  }
});
function followPointer() {
  if (!press || !press.dragging) return;
  api.dragMove();
  requestAnimationFrame(followPointer);
}
function release(click) {
  if (!press) return;
  const dragged = press.dragging;
  press = null;
  stage.classList.remove('dragging');
  if (dragged) api.dragEnd();
  else if (click) api.action('pet');
}
hit.addEventListener('pointerup', () => release(true));
hit.addEventListener('pointercancel', () => release(false));
hit.addEventListener('lostpointercapture', () => release(false));

window.addEventListener('contextmenu', (e) => {
  e.preventDefault();
  if (e.target.closest('[data-hit]')) api.action('menu');
});

$('bubble-main').addEventListener('click', () => api.action('bubble', bubbleSid));
$('ask-allow').addEventListener('click', () => { if (askSid) api.answerAsk(askSid, true); });
$('ask-deny').addEventListener('click', () => { if (askSid) api.answerAsk(askSid, false); });
$('c-chat').addEventListener('click', () => api.action('chat'));
$('c-activity').addEventListener('click', () => api.action('activity'));
$('c-more').addEventListener('click', () => api.action('menu'));

const BADGES = { needs_input: '!', ready: '✓', blocked: '✕' };

function bubbleText(o) {
  const top = o.top;
  if (!top) return null;
  let sub = top.title;
  if (o.status === 'running') sub = PetI18n.describeActivity(t, top.activity, false) || top.title || t('act.working');
  else if (o.status === 'needs_input') sub = PetI18n.describeActivity(t, top.activity, true) || top.message || top.title;
  if (o.count > 1) sub = `${sub} · ${t('more', { n: o.count - 1 })}`;
  return { status: t('status.' + o.status), sub };
}

function placeStage(pet, px) {
  const { MARGIN, TUCK } = PetSprites;
  const w = pet.base[0].length;
  const h = pet.base.length;
  stage.style.marginTop = -TUCK * px + 'px';
  Object.assign(hit.style, { left: MARGIN.x * px + 'px', top: MARGIN.top * px + 'px', width: w * px + 'px', height: h * px + 'px' });
  Object.assign($('shadow').style, {
    left: (MARGIN.x + w * 0.15) * px + 'px', width: w * 0.7 * px + 'px',
    top: (MARGIN.top + h - 1.4) * px + 'px', height: 2.4 * px + 'px',
  });
  Object.assign($('badge').style, { left: (MARGIN.x + w - 2) * px - 6 + 'px', top: MARGIN.top * px - 12 + 'px' });
}

function render() {
  const { overall: o, config: c, layout: l } = state;
  t = PetI18n.translator(state.strings);
  document.documentElement.lang = c.lang;
  document.documentElement.dir = state.dir;
  document.body.classList.toggle('mini', l.mini);

  const pet = state.pets.find((p) => p.id === l.petId);
  if (pet && !l.mini) {
    animator.setPet(pet);
    animator.setScale(l.px);
    animator.setState(o.sleeping ? 'sleeping' : o.status);
    placeStage(pet, l.px);
  }
  if (pet && !l.mini) animator.start(); else animator.stop();

  const asking = state.sessions.find((s) => state.asks[s.id]);
  const bubble = $('bubble');
  let text = bubbleText(o) || (!c.welcomed ? { status: t('welcome'), sub: '' } : null);
  if (asking) text = { status: t('status.needs_input'), sub: PetI18n.describeActivity(t, state.asks[asking.id], true) || asking.title };
  askSid = asking ? asking.id : null;
  bubbleSid = asking ? asking.id : (o.top ? o.top.id : null);
  bubble.hidden = !text;
  bubble.className = 's-' + (asking ? 'needs_input' : o.status);
  $('ask-buttons').hidden = !asking;
  $('ask-allow').title = t('ask.allow');
  $('ask-deny').title = t('ask.deny');
  if (text) {
    $('bubble-status').textContent = text.status;
    $('bubble-sub').textContent = text.sub || '';
  }

  const badge = $('badge');
  badge.hidden = !BADGES[o.status];
  badge.className = 's-' + o.status;
  badge.textContent = BADGES[o.status] || '';

  const attention = o.counts.needs_input + o.counts.blocked + o.counts.ready;
  $('count').hidden = attention === 0;
  $('count').textContent = attention > 9 ? '9+' : String(attention);

  $('c-chat').title = c.shortcutOk && c.shortcut ? `${t('tip.chat')} (${c.shortcut.replace(/\+/g, ' + ')})` : t('tip.chat');
  $('c-activity').title = t('tip.activity');
  $('c-more').title = t('tip.more');
}

let audio = null;
api.onChime((kind) => {
  try {
    audio = audio || new AudioContext();
    const notes = kind === 'blocked' ? [392, 294] : kind === 'needs_input' ? [660, 880] : [523, 784];
    notes.forEach((frequency, i) => {
      const at = audio.currentTime + i * 0.13;
      const osc = audio.createOscillator();
      const gain = audio.createGain();
      osc.frequency.value = frequency;
      gain.gain.setValueAtTime(0.0001, at);
      gain.gain.exponentialRampToValueAtTime(0.12, at + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, at + 0.28);
      osc.connect(gain).connect(audio.destination);
      osc.start(at);
      osc.stop(at + 0.3);
    });
  } catch {}
});

api.onState((s) => { state = s; render(); });
api.ready();
