'use strict';
const api = window.petAPI;
const $ = (id) => document.getElementById(id);
const card = $('card');

let state = null;
let t = PetI18n.translator({});
let view = { name: 'home', sid: null, since: 0 };
let sending = false;
let chatNote = '';
let connectError = '';
let importError = '';

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const baseName = (p) => String(p || '').split(/[\\/]/).filter(Boolean).pop() || '';
const firstLine = (s) => String(s || '').split('\n').find((l) => l.trim())?.trim() || '';

const lastHtml = new WeakMap();
function setHtml(el, html) {
  if (lastHtml.get(el) === html) return false;
  lastHtml.set(el, html);
  el.innerHTML = html;
  return true;
}
function setText(el, text) {
  if (el.textContent !== text) el.textContent = text;
}

function markdown(text) {
  const inline = (s) => esc(s)
    .replace(/`([^`\n]+)`/g, '<code dir="ltr">$1</code>')
    .replace(/\*\*([^*\n]+)\*\*/g, '<b>$1</b>')
    .replace(/^#{1,6}\s+(.+)$/gm, '<b>$1</b>')
    .replace(/^(\s*)[-*]\s+/gm, '$1• ')
    .replace(/\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g, '<a href="$2">$1</a>');
  return String(text).split('```').map((part, i) =>
    (i % 2 ? `<pre dir="ltr"><code>${esc(part.replace(/^[\w+#.-]*\n/, ''))}</code></pre>` : inline(part.replace(/^\n+|\n+$/g, '')))).join('');
}

function textDir(text) {
  const plain = String(text || '').replace(/```[\s\S]*?```/g, '').replace(/`[^`\n]*`/g, '');
  const rtl = (plain.match(/[֐-׿؀-ۿ]/g) || []).length;
  const ltr = (plain.match(/[A-Za-z]/g) || []).length;
  if (!rtl && !ltr) return 'auto';
  return rtl >= ltr * 0.4 ? 'rtl' : 'ltr';
}

function typedDir(text) {
  const strong = /[A-Za-zÀ-ɏ֐-׿؀-ۿ]/.exec(text || '');
  if (!strong) return document.documentElement.dir || 'ltr';
  return /[֐-׿؀-ۿ]/.test(strong[0]) ? 'rtl' : 'ltr';
}
function syncDir(el) {
  const dir = typedDir(el.value);
  if (el.dir !== dir) el.dir = dir;
}
const TEXT_BOXES = [$('prompt'), $('reply'), $('hatch-desc')];
for (const box of TEXT_BOXES) box.addEventListener('input', () => syncDir(box));

function relTime(ts) {
  if (!ts) return '';
  const minutes = Math.floor(Math.max(0, state.now - ts) / 60e3);
  if (minutes < 1) return t('activity.now');
  if (minutes < 60) return t('activity.min', { n: minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t('activity.hour', { n: hours });
  return t('activity.day', { n: Math.floor(hours / 24) });
}

function show(next) {
  view = { name: next.name, sid: next.sid || null, since: Date.now() };
  card.dataset.view = view.name;
  api.setView(view.name);
  if (state) render();
  if (view.name === 'chat' || next.focus) setTimeout(() => $('prompt').focus(), 40);
}

function folderChoices() {
  const seen = new Set();
  const out = [];
  const add = (dir) => {
    if (!dir || seen.has(dir.toLowerCase())) return;
    seen.add(dir.toLowerCase());
    out.push(dir);
  };
  add(state.config.quickCwd);
  for (const s of state.sessions) if (s.origin !== 'quick' && !s.noFolder) add(s.cwd);
  return out.slice(0, 7);
}

function renderComposer() {
  const c = state.config;
  $('prompt').placeholder = t('chat.placeholder');
  $('send').title = t('chat.send');
  $('send').disabled = sending;

  const options = [`<option value="">${esc(t('chat.noFolder'))}</option>`]
    .concat(folderChoices().map((dir) => `<option value="${esc(dir)}" title="${esc(dir)}">${esc(baseName(dir))}</option>`))
    .concat(`<option value="__pick__">${esc(t('chat.choose'))}</option>`).join('');
  if (document.activeElement !== $('cwd')) {
    setHtml($('cwd'), options);
    $('cwd').value = c.quickCwd || '';
  }

  setText($('mode-note'), c.quickMode === 'terminal' ? t('chat.terminalMode') : '');
  const note = !state.claudeFound ? t('chat.claudeMissing') : chatNote;
  $('chat-note').hidden = !note;
  setText($('chat-note'), note || '');
}

async function sendChat() {
  const prompt = $('prompt').value.trim();
  if (!prompt || sending) return;
  sending = true;
  renderComposer();
  const res = await api.sendChat({ prompt, cwd: state.config.quickCwd });
  sending = false;
  if (!res.ok) {
    chatNote = res.error === 'claude-not-found' ? t('chat.claudeMissing') : String(res.error || '');
    renderComposer();
    return;
  }
  $('prompt').value = '';
  chatNote = res.pasted ? t('chat.pasted') : '';
  if (res.sid) show({ name: 'thread', sid: res.sid });
  else if (res.pasted) renderComposer();
  else api.close();
}

function itemSub(s) {
  let detail = '';
  if (s.status === 'running') detail = PetI18n.describeActivity(t, s.activity, false);
  else if (s.status === 'needs_input') detail = PetI18n.describeActivity(t, s.activity, true) || s.message;
  else if (s.status === 'ready' || s.status === 'blocked') detail = firstLine(s.message);
  else detail = s.noFolder ? t('chat.noFolder') : s.folder;
  return `<b>${esc(t('status.' + s.status))}</b>${detail ? ' · <bdi>' + esc(detail) + '</bdi>' : ''}`;
}

function renderHome() {
  setText($('activity-title'), t('activity.title'));
  setText($('clear'), t('activity.clear'));
  $('clear').hidden = !state.sessions.some((s) => s.status === 'ready' || s.status === 'blocked' || (s.status === 'idle' && !s.live));
  $('open-settings').title = t('settings.title');

  const conn = state.connection;
  $('connect-card').hidden = conn.connected;
  setText($('connect-title'), t('connect.title'));
  setText($('connect-body'), conn.partial ? t('connect.partial') : t('connect.body'));
  setText($('connect-detail'), t('connect.detail', { file: PetI18n.isolateLtr(conn.settingsFile) }));
  setText($('connect-btn'), t('connect.button'));
  setText($('connect-error'), connectError || (conn.node ? '' : t('connect.noNode')));

  const items = state.sessions.map((s) => `
    <button class="item s-${s.status}" data-sid="${esc(s.id)}" type="button">
      <span class="dot"></span>
      <span class="item-main">
        <span class="item-title" dir="auto">${esc(s.title || s.folder || 'Claude Code')}</span>
        <span class="item-sub" dir="auto">${itemSub(s)}</span>
      </span>
      <span class="item-time">${esc(relTime(s.updatedAt))}</span>
    </button>`).join('');
  setHtml($('list'), items || `<div class="empty">${esc(t('activity.empty'))}</div>`);
}

function renderThread() {
  const s = state.sessions.find((x) => x.id === view.sid) || view.last;
  if (!s) {
    if (Date.now() - view.since > 3000) show({ name: 'home' });
    return;
  }
  view.last = s;
  const seenKey = s.id + '|' + s.statusSince;
  if (s.unread && document.hasFocus() && view.seenKey !== seenKey) {
    view.seenKey = seenKey;
    api.seen(s.id);
  }

  setText($('thread-title'), s.title || s.folder || 'Claude Code');
  $('thread-chip').className = 'chip s-' + s.status;
  setText($('thread-chip'), t('status.' + (s.status === 'idle' && s.ended ? 'ended' : s.status)));
  const meta = [];
  if (s.origin === 'quick') meta.push(t('thread.quick'));
  else if (s.surface && !t('surface.' + s.surface).startsWith('surface.')) meta.push(t('surface.' + s.surface));
  if (s.noFolder) meta.push(t('chat.noFolder'));
  else if (s.folder && s.folder !== 'quick-chat') meta.push(PetI18n.isolate(s.folder));
  meta.push(relTime(s.updatedAt));
  setText($('thread-meta'), meta.filter(Boolean).join(' · '));
  $('thread-meta').title = s.cwd || '';

  let html = '';
  if (s.lastPrompt) html += `<div class="msg you"><label>${esc(t('thread.you'))}</label><div class="text" dir="${textDir(s.lastPrompt)}">${esc(s.lastPrompt)}</div></div>`;
  const answer = s.result || s.message;
  const working = s.status === 'running';
  $('thread-working').hidden = !working;
  if (working) setText($('thread-working-text'), PetI18n.describeActivity(t, s.activity, false) || t('act.working'));
  if (working) {
  } else if (s.status === 'needs_input') {
    const what = PetI18n.describeActivity(t, s.activity, true) || s.message;
    const buttons = state.asks[s.id]
      ? `<div class="row ask-row"><button data-ask="allow" class="primary" type="button">${esc(t('ask.allow'))}</button><button data-ask="deny" type="button">${esc(t('ask.deny'))}</button></div>`
      : '';
    html += `<div class="waiting"><b>${esc(t('thread.waiting'))}</b>${what ? `<div dir="auto">${esc(what)}</div>` : ''}${buttons}</div>`;
  } else if (answer) {
    html += `<div class="msg claude"><label>${esc(t('thread.claude'))}</label><div class="text md${s.status === 'blocked' ? ' error' : ''}" dir="${textDir(answer)}">${markdown(answer)}</div></div>`;
  } else if (!s.lastPrompt || s.status === 'ready' || s.status === 'blocked') {
    html += `<div class="empty">${esc(t('thread.noMessage'))}</div>`;
  }
  setHtml($('thread-messages'), html);

  const actions = [];
  if (state.claudeFound && !s.live && s.status !== 'running') actions.push(`<button data-act="terminal" type="button">${esc(t('thread.terminal'))}</button>`);
  if (s.live && s.surface === 'claude-desktop' && state.canOpenClaudeApp) actions.push(`<button data-act="claude" type="button">${esc(t('thread.openClaude'))}</button>`);
  if (s.cwd && s.origin !== 'quick' && !s.noFolder) actions.push(`<button data-act="folder" type="button">${esc(t('thread.folder'))}</button>`);
  actions.push(`<span class="spacer"></span><button data-act="dismiss" class="ghost" type="button">${esc(t('thread.dismiss'))}</button>`);
  setHtml($('thread-actions'), actions.join(''));

  $('reply-row').hidden = !s.canReply || !state.claudeFound;
  $('reply').placeholder = t('chat.reply');
}

async function sendReply() {
  const prompt = $('reply').value.trim();
  if (!prompt) return;
  $('reply').value = '';
  syncDir($('reply'));
  autosize($('reply'));
  await api.reply({ sid: view.sid, prompt });
}

function renderPetGrid() {
  const c = state.config;
  const signature = JSON.stringify([state.pets.map((p) => [p.id, p.name]), c.pet, c.lang]);
  if (lastHtml.get($('pet-grid')) === signature) return;
  lastHtml.set($('pet-grid'), signature);

  const grid = $('pet-grid');
  grid.textContent = '';
  for (const pet of state.pets) {
    const tile = document.createElement('button');
    tile.type = 'button';
    tile.className = 'pet-tile' + (c.pet === pet.id ? ' selected' : '');
    tile.dataset.pet = pet.id;
    const canvas = document.createElement('canvas');
    const side = Math.max(pet.base.length, pet.base[0].length) + 2;
    PetSprites.drawStill(canvas, pet, 'base', Math.max(1, Math.floor(50 / side)));
    const name = document.createElement('span');
    name.textContent = pet.name;
    tile.append(canvas, name);
    if (pet.custom) {
      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'remove';
      remove.dataset.remove = pet.id;
      remove.title = t('settings.delete');
      remove.textContent = '×';
      tile.append(remove);
    }
    grid.append(tile);
  }
  const mini = document.createElement('button');
  mini.type = 'button';
  mini.className = 'pet-tile' + (c.pet === 'mini' ? ' selected' : '');
  mini.dataset.pet = 'mini';
  mini.innerHTML = `<div class="mini-art"><i></i><i></i><i></i></div><span>${esc(t('settings.mini'))}</span>`;
  grid.append(mini);
}

function renderSettings() {
  const c = state.config;
  const conn = state.connection;
  setText($('settings-title'), t('settings.title'));
  setText($('l-pets'), t('settings.pets'));
  renderPetGrid();

  $('hatch-desc').placeholder = t('settings.createPlaceholder');
  setText($('hatch'), state.hatching ? t('settings.hatching') : t('settings.hatch'));
  $('hatch').disabled = state.hatching || !state.claudeFound;
  $('hatch-desc').disabled = state.hatching;
  $('refresh').title = t('settings.refresh');
  const failed = state.lastHatch && !state.lastHatch.ok;
  $('hatch-note').hidden = !failed;
  $('hatch-note').className = 'small error';
  if (failed) {
    const e = state.lastHatch.error === 'claude-not-found' ? t('chat.claudeMissing') : state.lastHatch.error;
    setText($('hatch-note'), t('settings.hatchFailed', { e }));
  }

  setText($('l-size'), t('settings.size'));
  setText($('size-reset'), t('settings.reset'));
  if (document.activeElement !== $('size')) $('size').value = String(c.size);

  setText($('l-lang'), t('settings.language'));
  setHtml($('lang'), [`<option value="auto">${esc(t('settings.lang.auto'))}</option>`]
    .concat(state.languages.map((l) => `<option value="${esc(l.code)}">${esc(l.name)}</option>`)).join(''));
  $('lang').value = c.langSetting;

  setText($('l-quick'), t('settings.quickMode'));
  setHtml($('quick-mode'), `<option value="background">${esc(t('settings.quickMode.background'))}</option><option value="terminal">${esc(t('settings.quickMode.terminal'))}</option>`);
  $('quick-mode').value = c.quickMode;

  setText($('l-shortcut'), t('settings.shortcut'));
  $('shortcut').value = c.shortcut ? c.shortcut.replace(/\+/g, ' + ') : '';
  $('shortcut').placeholder = t('settings.shortcutHint');
  $('shortcut-note').hidden = c.shortcutOk || !c.shortcut;
  setText($('shortcut-note'), t('settings.shortcutBad'));

  setText($('l-conn'), t('settings.connection'));
  $('conn-dot').className = 'dot' + (conn.connected ? ' s-ready' : conn.partial ? ' s-needs_input' : '');
  setText($('conn-status'), conn.connected ? t('connect.connected') : conn.partial ? t('connect.partial') : t('connect.notConnected'));
  setText($('conn-btn'), conn.connected ? t('connect.disconnect') : t('connect.button'));
  $('conn-btn').className = conn.connected ? '' : 'primary';
  $('conn-detail').hidden = conn.connected;
  setText($('conn-detail'), conn.connected ? '' : t('connect.detail', { file: PetI18n.isolateLtr(conn.settingsFile) }));
  const error = connectError || (conn.node ? '' : t('connect.noNode'));
  $('conn-error').hidden = !error;
  setText($('conn-error'), error);

  setText($('import-pet'), t('settings.import'));
  setText($('export-pet'), t('settings.export'));
  $('export-pet').disabled = c.pet === 'mini';
  $('import-note').hidden = !importError;
  setText($('import-note'), importError ? t('settings.importFailed', { e: importError }) : '');

  const checks = [
    ['show-background', 'l-background', 'settings.background', c.showBackground],
    ['sound', 'l-sound', 'settings.sound', c.sound],
    ['notify', 'l-notify', 'settings.notify', c.notify],
    ['approvals', 'l-approvals', 'settings.approvals', c.approvals],
    ['store-text', 'l-store-text', 'settings.storeText', c.storeText],
  ];
  for (const [box, label, key, value] of checks) {
    setText($(label), t(key));
    $(box).checked = !!value;
  }
  setText($('background-note'), t('settings.backgroundNote'));
  setText($('l-alerts'), t('settings.alerts'));
  setText($('alerts-note'), t('settings.alertsNote'));
  setText($('approvals-note'), t('settings.approvalsNote'));
  setText($('l-privacy'), t('settings.privacy'));
  setText($('store-text-note'), t('settings.storeTextNote'));
  setText($('clear-history'), t('settings.clearHistory'));
  setText($('cleared'), t('settings.cleared'));
  setText($('about'), t('settings.about', { name: state.brand }));

  setText($('off-note'), t('settings.offNote'));
  setText($('turn-off'), t('settings.off'));
}

async function toggleConnection() {
  connectError = '';
  const res = state.connection.connected ? await api.disconnect() : await api.connect();
  if (res && res.error) connectError = t('connect.error', { e: res.error });
  render();
}

function shortcutKey(e) {
  const code = e.code;
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  const named = {
    Space: 'Space', Enter: 'Enter', Tab: 'Tab', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right',
    Backquote: '`', Minus: '-', Equal: '=', Comma: ',', Period: '.', Slash: '/', Semicolon: ';', BracketLeft: '[', BracketRight: ']',
  };
  return named[code] || null;
}

function render() {
  const c = state.config;
  t = PetI18n.translator(state.strings);
  document.documentElement.lang = c.lang;
  document.documentElement.dir = state.dir;
  for (const box of TEXT_BOXES) syncDir(box);
  if (view.name === 'chat' || view.name === 'home') renderComposer();
  if (view.name === 'home') renderHome();
  if (view.name === 'thread') renderThread();
  if (view.name === 'settings') renderSettings();
}

function autosize(el) {
  el.style.height = 'auto';
  el.style.height = Math.min(el.scrollHeight + 2, parseInt(getComputedStyle(el).maxHeight, 10) || 120) + 'px';
}

const isSubmit = (e) => e.key === 'Enter' && !e.shiftKey && !e.isComposing;

$('prompt').addEventListener('keydown', (e) => { if (isSubmit(e)) { e.preventDefault(); sendChat(); } });
$('send').addEventListener('click', sendChat);
$('reply').addEventListener('keydown', (e) => { if (isSubmit(e)) { e.preventDefault(); sendReply(); } });
$('reply').addEventListener('input', () => autosize($('reply')));
$('reply-send').addEventListener('click', sendReply);

$('cwd').addEventListener('change', async () => {
  let dir = $('cwd').value;
  if (dir === '__pick__') {
    dir = await api.pickFolder();
    if (dir === null) { $('cwd').value = state.config.quickCwd || ''; return; }
  }
  api.setConfig({ quickCwd: dir });
  $('cwd').blur();
});

$('list').addEventListener('click', (e) => {
  const item = e.target.closest('[data-sid]');
  if (item) show({ name: 'thread', sid: item.dataset.sid });
});
$('clear').addEventListener('click', () => api.clearFinished());
$('open-settings').addEventListener('click', () => show({ name: 'settings' }));
$('connect-btn').addEventListener('click', toggleConnection);
$('conn-btn').addEventListener('click', toggleConnection);
for (const back of document.querySelectorAll('.back')) back.addEventListener('click', () => show({ name: 'home' }));

$('thread-actions').addEventListener('click', (e) => {
  const act = e.target.closest('[data-act]')?.dataset.act;
  if (act === 'terminal') api.openTerminal(view.sid);
  else if (act === 'claude') api.openClaudeApp();
  else if (act === 'folder') api.openFolder(view.sid);
  else if (act === 'dismiss') { api.dismiss(view.sid); show({ name: 'home' }); }
});
$('thread-body').addEventListener('click', (e) => {
  const ask = e.target.closest('[data-ask]');
  if (ask) { api.answerAsk(view.sid, ask.dataset.ask === 'allow'); return; }
  const link = e.target.closest('a[href]');
  if (!link) return;
  e.preventDefault();
  api.openLink(link.getAttribute('href'));
});

$('pet-grid').addEventListener('click', (e) => {
  const remove = e.target.closest('[data-remove]');
  if (remove) { e.stopPropagation(); api.deletePet(remove.dataset.remove); return; }
  const tile = e.target.closest('[data-pet]');
  if (tile) api.setConfig({ pet: tile.dataset.pet });
});
async function hatch() {
  const description = $('hatch-desc').value.trim();
  if (!description) { $('hatch-desc').focus(); return; }
  const res = await api.hatch(description);
  if (res && res.ok) { $('hatch-desc').value = ''; syncDir($('hatch-desc')); }
}
$('hatch').addEventListener('click', hatch);
$('hatch-desc').addEventListener('keydown', (e) => { if (isSubmit(e)) { e.preventDefault(); hatch(); } });
$('refresh').addEventListener('click', () => api.refreshPets());
$('size').addEventListener('input', () => api.setConfig({ size: Number($('size').value) }));
$('size-reset').addEventListener('click', () => api.setConfig({ size: 1 }));
$('lang').addEventListener('change', () => api.setConfig({ lang: $('lang').value }));
for (const [box, key] of [['show-background', 'showBackground'], ['sound', 'sound'], ['notify', 'notify'], ['approvals', 'approvals'], ['store-text', 'storeText']]) {
  $(box).addEventListener('change', () => api.setConfig({ [key]: $(box).checked }));
}
$('clear-history').addEventListener('click', () => {
  api.clearHistory();
  $('cleared').hidden = false;
  setTimeout(() => { $('cleared').hidden = true; }, 2500);
});
$('import-pet').addEventListener('click', async () => {
  const res = await api.importPet();
  importError = res && !res.ok && !res.cancelled ? String(res.error || '') : '';
  render();
});
$('export-pet').addEventListener('click', () => api.exportPet(state.config.pet));
$('quick-mode').addEventListener('change', () => api.setConfig({ quickMode: $('quick-mode').value }));
$('turn-off').addEventListener('click', () => api.turnOff());

$('shortcut').addEventListener('keydown', (e) => {
  if (e.key === 'Tab') return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === 'Escape') { $('shortcut').blur(); return; }
  if (e.key === 'Backspace' || e.key === 'Delete') { api.setConfig({ shortcut: '' }); return; }
  const mods = [];
  if (e.ctrlKey) mods.push('Control');
  if (e.altKey) mods.push('Alt');
  if (e.shiftKey) mods.push('Shift');
  if (e.metaKey) mods.push('Super');
  const key = shortcutKey(e);
  if (key && mods.length) api.setConfig({ shortcut: [...mods, key].join('+') });
});

window.addEventListener('keydown', (e) => {
  if (e.key !== 'Escape') return;
  if (view.name === 'thread' || view.name === 'settings') show({ name: 'home' });
  else api.close();
});
window.addEventListener('focus', () => { if (state) render(); });

api.onState((s) => { state = s; render(); });
api.onShow((next) => show(next));
api.ready();
