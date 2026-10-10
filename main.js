'use strict';
const { app, BrowserWindow, ipcMain, screen, Tray, Menu, Notification, globalShortcut, nativeImage, shell, clipboard, dialog } = require('electron');
const path = require('path');
const fs = require('fs');
const os = require('os');
const crypto = require('crypto');

const paths = require('./lib/paths');
const configStore = require('./lib/config');
const { SessionStore, sanitize } = require('./lib/store');
const connect = require('./lib/connect');
const petsLib = require('./lib/pets');
const quick = require('./lib/quickchat');
const control = require('./lib/control');
const presence = require('./lib/presence');
const locales = require('./lib/locales');
const brand = require('./lib/brand');
const sprites = require('./ui/sprites.js');
const i18n = require('./ui/i18n.js');

const DISCONNECT_ONLY = process.argv.includes('--disconnect');
if (DISCONNECT_ONLY) {
  try { connect.disconnect(); } catch {}
  app.exit(0);
}

app.setPath('userData', path.join(paths.PET_HOME, 'app-data'));
app.setAppUserModelId(brand.APP_ID);

const gotLock = !DISCONNECT_ONLY && app.requestSingleInstanceLock();
if (!gotLock) app.quit();

const PANEL_WIDTH = 380;
const PANEL_HEIGHTS = { chat: 196, home: 520, thread: 520, settings: 600 };
const BUBBLE_ZONE = 62;
const CONTROLS_ZONE = 50;
const SLEEP_AFTER = 10 * 60e3;

let config = configStore.load();
let pets = [];
let petWin = null;
let panelWin = null;
let tray = null;
let connection = { connected: false, partial: false, node: true, error: null, settingsFile: paths.SETTINGS_FILE };
let hatching = false;
let lastHatch = null;
let shortcutOk = true;
let panelView = 'home';
let keepPanelOpen = false;
let panelAfterMenu = null;
let menuOpen = false;
let drag = null;
let pointerTimer = null;
let pointerInside = false;
let petIgnoring = true;
let broadcastTimer = null;
const asks = new Map();

const store = new SessionStore({ eventsDir: paths.EVENTS_DIR, registryDir: paths.REGISTRY_DIR });
store.showBackground = !!config.showBackground;

const tables = locales.loadAll();
function language() {
  return locales.pick(tables, config.lang, app.getPreferredSystemLanguages());
}
const strings = () => locales.strings(tables, language());
const t = (key, vars) => i18n.translator(strings())(key, vars);

try {
  app.commandLine.appendSwitch('lang', strings().$dir === 'rtl' ? language() : 'en-US');
} catch {}

function tildePath(file) {
  const home = os.homedir();
  return file && file.startsWith(home) ? '~' + file.slice(home.length).split(path.sep).join('/') : file;
}

function currentPet() {
  if (config.pet === 'mini') return null;
  return pets.find((p) => p.id === config.pet) || pets.find((p) => p.id === 'ember') || pets[0] || null;
}

function layout() {
  const pet = currentPet();
  if (!pet) return { mini: true, petId: null, px: 0, width: 250, height: BUBBLE_ZONE + CONTROLS_ZONE + 6 };
  const m = sprites.metrics(pet);
  const px = Math.max(1, Math.round((96 * config.size) / Math.max(m.w, m.h)));
  return { mini: false, petId: pet.id, px, width: Math.max(250, m.lw * px + 40), height: BUBBLE_ZONE + (m.lh - sprites.TUCK) * px + CONTROLS_ZONE };
}

function defaultAnchor() {
  const wa = screen.getPrimaryDisplay().workArea;
  return { x: wa.x + wa.width - 170, y: wa.y + wa.height - 6 };
}

function petBounds() {
  const l = layout();
  let a = config.anchor;
  const onScreen = a && screen.getAllDisplays().some((d) => {
    const b = d.bounds;
    return a.x >= b.x && a.x <= b.x + b.width && a.y - 40 >= b.y && a.y - 40 <= b.y + b.height;
  });
  if (!onScreen) a = config.anchor = defaultAnchor();
  return { x: Math.round(a.x - l.width / 2), y: Math.round(a.y - l.height), width: l.width, height: l.height };
}

function saveConfig() {
  configStore.save(config);
}

function publicState() {
  const snap = store.snapshot();
  const now = Date.now();
  const overall = snap.overall;
  const sleeping = overall.status === 'idle' && now - (overall.lastActivity || 0) > SLEEP_AFTER;
  return {
    now,
    sessions: snap.sessions.slice(0, 40),
    overall: { ...overall, sleeping },
    config: {
      pet: config.pet, size: config.size, lang: language(), langSetting: config.lang,
      shortcut: config.shortcut, shortcutOk, quickMode: config.quickMode, quickCwd: config.quickCwd,
      welcomed: config.welcomed,
      showBackground: config.showBackground, storeText: config.storeText,
      sound: config.sound, notify: config.notify, approvals: config.approvals,
    },
    brand: brand.NAME,
    strings: strings(),
    dir: strings().$dir === 'rtl' ? 'rtl' : 'ltr',
    languages: locales.list(tables),
    asks: Object.fromEntries([...asks].map(([sid, ask]) => [sid, { tool: ask.tool, detail: ask.detail }])),
    layout: layout(),
    pets,
    connection: { ...connection, settingsFile: tildePath(connection.settingsFile) },
    hatching,
    lastHatch,
    claudeFound: !!quick.findClaude(),
    canOpenClaudeApp: !!app.getApplicationNameForProtocol('claude://'),
    platform: process.platform,
  };
}

function broadcast() {
  if (broadcastTimer) return;
  broadcastTimer = setTimeout(() => {
    broadcastTimer = null;
    const state = publicState();
    for (const win of [petWin, panelWin]) {
      if (win && !win.isDestroyed()) win.webContents.send('state', state);
    }
    updateTray(state);
  }, 60);
}

function webPreferences(keepRendering) {
  return { preload: path.join(__dirname, 'preload.js'), contextIsolation: true, nodeIntegration: false, sandbox: true, backgroundThrottling: !keepRendering };
}

function appIcon() {
  const pet = currentPet() || pets.find((p) => p.id === 'ember') || pets[0];
  if (!pet) return undefined;
  return nativeImage.createFromBitmap(petsLib.trayBitmap(pet, 64), { width: 64, height: 64 });
}

function overlayOptions() {
  return {
    show: false, frame: false, transparent: true, backgroundColor: '#00000000', hasShadow: false,
    resizable: false, minimizable: false, maximizable: false, fullscreenable: false,
    skipTaskbar: true, alwaysOnTop: true, icon: appIcon(),
    ...(process.platform === 'win32' ? { type: 'toolbar' } : {}),
  };
}

function createPetWindow() {
  petWin = new BrowserWindow({
    ...overlayOptions(),
    ...petBounds(),
    focusable: false,
    webPreferences: webPreferences(true),
  });
  petWin.setAlwaysOnTop(true, 'screen-saver');
  petWin.setIgnoreMouseEvents(true);
  petWin.loadFile(path.join(__dirname, 'ui', 'pet.html'));
  petWin.once('ready-to-show', () => { petWin.showInactive(); petWin.setSkipTaskbar(true); });
  petWin.on('closed', () => { petWin = null; });
}

function createPanelWindow() {
  panelWin = new BrowserWindow({
    ...overlayOptions(),
    width: PANEL_WIDTH, height: PANEL_HEIGHTS.home,
    webPreferences: webPreferences(false),
  });
  panelWin.setAlwaysOnTop(true, 'screen-saver');
  panelWin.loadFile(path.join(__dirname, 'ui', 'panel.html'));
  panelWin.on('blur', () => { if (!keepPanelOpen) hidePanel(); });
  panelWin.on('closed', () => { panelWin = null; });
  panelWin.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  panelWin.webContents.on('will-navigate', (e) => e.preventDefault());
}

function positionPanel(view) {
  if (!panelWin || !petWin) return;
  const height = PANEL_HEIGHTS[view] || PANEL_HEIGHTS.home;
  const pet = petWin.getBounds();
  const wa = screen.getDisplayMatching(pet).workArea;
  const petCentre = pet.x + pet.width / 2;
  const gap = -18;
  let x = petCentre > wa.x + wa.width / 2 ? pet.x - PANEL_WIDTH - gap : pet.x + pet.width + gap;
  x = Math.min(Math.max(x, wa.x + 8), wa.x + wa.width - PANEL_WIDTH - 8);
  let y = pet.y + pet.height - height - 8;
  y = Math.min(Math.max(y, wa.y + 8), wa.y + wa.height - height - 8);
  panelWin.setBounds({ x: Math.round(x), y: Math.round(y), width: PANEL_WIDTH, height });
}

function showPanel(view, extra = {}) {
  if (!panelWin) return;
  panelView = view;
  positionPanel(view);
  panelWin.webContents.send('panel:show', { name: view, ...extra });
  panelWin.show();
  panelWin.setSkipTaskbar(true);
  panelWin.focus();
  panelWin.webContents.focus();
}

function hidePanel() {
  if (panelWin && panelWin.isVisible()) panelWin.hide();
}

function togglePanel(view, extra) {
  if (panelWin && panelWin.isVisible() && panelView === view) hidePanel();
  else showPanel(view, extra);
}

function turnOff() {
  config.visible = false;
  saveConfig();
  setTimeout(() => app.quit(), 80);
}

function resizePet() {
  if (petWin) petWin.setBounds(petBounds());
}

function trayImage() {
  const pet = currentPet() || pets.find((p) => p.id === 'ember') || pets[0];
  if (!pet) return nativeImage.createEmpty();
  return nativeImage.createFromBitmap(petsLib.trayBitmap(pet, 32), { width: 32, height: 32 });
}

function menuTemplate() {
  const petItems = pets.map((p) => ({
    label: p.name, type: 'radio', checked: config.pet === p.id, click: () => setConfig({ pet: p.id }),
  }));
  petItems.push({ label: t('settings.mini'), type: 'radio', checked: config.pet === 'mini', click: () => setConfig({ pet: 'mini' }) });
  return [
    { label: t('menu.chat'), accelerator: shortcutOk ? config.shortcut : undefined, registerAccelerator: false, click: fromMenu('chat') },
    { label: t('menu.activity'), click: fromMenu('home') },
    { type: 'separator' },
    { label: t('menu.pet'), submenu: petItems },
    { label: t('menu.settings'), click: fromMenu('settings') },
    { type: 'separator' },
    { label: t('menu.off'), click: turnOff },
  ];
}

let trayKey = '';
function updateTray(state) {
  if (!tray) return;
  const key = [config.pet, language(), shortcutOk, config.shortcut, pets.length].join('|');
  if (key !== trayKey) {
    trayKey = key;
    tray.setImage(trayImage());
    tray.setContextMenu(Menu.buildFromTemplate(menuTemplate()));
  }
  const o = state.overall;
  tray.setToolTip(o.status === 'idle' ? brand.NAME : `${brand.NAME} · ${t('status.' + o.status)}${o.count > 1 ? ` (${o.count})` : ''}`);
}

function createTray() {
  tray = new Tray(trayImage());
  tray.on('click', () => togglePanel('home'));
}

function fromMenu(view) {
  return () => {
    if (menuOpen) panelAfterMenu = view;
    else showPanel(view);
  };
}

function popupPetMenu() {
  if (!petWin) return;
  keepPanelOpen = true;
  menuOpen = true;
  petWin.setFocusable(true);
  Menu.buildFromTemplate(menuTemplate()).popup({
    window: petWin,
    callback: () => {
      menuOpen = false;
      const view = panelAfterMenu;
      panelAfterMenu = null;
      if (view) showPanel(view);
      if (petWin) {
        if (panelWin && panelWin.isVisible()) petWin.moveTop();
        petWin.setFocusable(false);
        petWin.setSkipTaskbar(true);
      }
      setTimeout(() => {
        if (panelWin && panelWin.isVisible() && !panelWin.isFocused()) panelWin.focus();
        keepPanelOpen = false;
      }, 150);
    },
  });
}

function registerShortcut() {
  globalShortcut.unregisterAll();
  shortcutOk = true;
  if (!config.shortcut) return;
  try {
    shortcutOk = globalShortcut.register(config.shortcut, () => showPanel('chat'));
  } catch {
    shortcutOk = false;
  }
}

function setConfig(patch) {
  const before = { ...config };
  if (typeof patch.pet === 'string' && (patch.pet === 'mini' || pets.some((p) => p.id === patch.pet))) config.pet = patch.pet;
  if (typeof patch.size === 'number' && Number.isFinite(patch.size)) config.size = Math.min(2, Math.max(0.6, Math.round(patch.size * 20) / 20));
  if (['background', 'terminal'].includes(patch.quickMode)) config.quickMode = patch.quickMode;
  if (typeof patch.quickCwd === 'string') config.quickCwd = patch.quickCwd;
  if (typeof patch.shortcut === 'string') config.shortcut = patch.shortcut.slice(0, 60);
  if (typeof patch.welcomed === 'boolean') config.welcomed = patch.welcomed;
  for (const flag of ['showBackground', 'storeText', 'sound', 'notify', 'approvals']) {
    if (typeof patch[flag] === 'boolean') config[flag] = patch[flag];
  }
  if (patch.lang === 'auto' || (typeof patch.lang === 'string' && Object.prototype.hasOwnProperty.call(tables, patch.lang))) config.lang = patch.lang;
  saveConfig();
  store.showBackground = !!config.showBackground;
  if (config.pet !== before.pet || config.size !== before.size) resizePet();
  if (config.shortcut !== before.shortcut) registerShortcut();
  if (config.lang !== before.lang) writeAppInfo();
  if (!config.approvals) for (const sid of [...asks.keys()]) settleAsk(sid, null);
  broadcast();
}

function writeAppInfo() {
  const texts = {};
  for (const key of ['on', 'off', 'started', 'chat', 'notInstalled', 'failed']) texts[key] = t('cli.' + key);
  try {
    control.writeAppInfo({ exe: process.execPath, args: app.isPackaged ? [] : [paths.APP_DIR], lang: language(), texts });
  } catch {}
}

function runCommand(command, payload, connection) {
  if (command === 'ping') return { ok: true };
  if (command === 'off') { turnOff(); return { ok: true }; }
  if (command === 'chat') { showPanel('chat'); return { ok: true }; }
  if (command === 'ask') return askUser(payload || {}, connection);
  return { ok: false };
}

function askUser(payload, connection) {
  const sid = sanitize(payload.sid);
  if (!sid || !config.approvals) return { ok: true, decision: null };
  store.readLog(sid + '.jsonl');
  settleAsk(sid, null);
  return new Promise((resolve) => {
    const ask = {
      tool: String(payload.tool || '').slice(0, 80),
      detail: String(payload.detail || '').slice(0, 300),
      answer: (decision) => resolve({ ok: true, decision }),
    };
    asks.set(sid, ask);
    connection.onClose(() => { if (asks.get(sid) === ask) { asks.delete(sid); broadcast(); } });
    broadcast();
  });
}

function settleAsk(sid, decision) {
  const ask = asks.get(sid);
  if (!ask) return;
  asks.delete(sid);
  ask.answer(decision);
  broadcast();
}

function dropAnsweredAsks() {
  for (const sid of [...asks.keys()]) {
    const session = store.get(sid);
    if (!session || session.status !== 'needs_input') settleAsk(sid, null);
  }
}

const alerted = new Map();
let alertsArmed = false;

function announceChanges() {
  const worth = new Set(['needs_input', 'ready', 'blocked']);
  let loudest = null;
  for (const s of store.snapshot().sessions) {
    const was = alerted.get(s.id);
    alerted.set(s.id, s.status);
    if (!alertsArmed || was === s.status || !worth.has(s.status)) continue;
    if (!loudest || s.status === 'needs_input') loudest = s;
  }
  if (!loudest) return;
  if (config.sound && petWin && !petWin.isDestroyed()) petWin.webContents.send('pet:chime', loudest.status);
  if (config.notify && Notification.isSupported()) {
    const body = loudest.title || (loudest.noFolder ? '' : loudest.folder) || brand.NAME;
    const note = new Notification({ title: t('alert.' + loudest.status), body, silent: true, icon: appIcon() });
    note.on('click', () => showPanel('thread', { sid: loudest.id }));
    note.show();
  }
}

function startQuickChat({ prompt, cwd }) {
  prompt = String(prompt || '').trim();
  if (!prompt) return { ok: false, error: 'empty' };
  if (!quick.findClaude()) return { ok: false, error: 'claude-not-found' };
  const dir = cwd || paths.QUICK_DIR;

  if (config.quickMode === 'terminal') {
    const res = quick.chatInTerminal({ cwd: dir, prompt });
    if (res.pasted) clipboard.writeText(prompt);
    return res;
  }

  const sid = crypto.randomUUID();
  store.record(sid, { ev: 'QuickChat', prompt: prompt.slice(0, 2000), cwd: dir });
  runAndRecord(sid, quick.runPrint({ prompt, cwd: dir, sessionId: sid }));
  return { ok: true, sid };
}

function replyToChat({ sid, prompt }) {
  prompt = String(prompt || '').trim();
  const session = store.get(sid);
  if (!prompt || !session || session.origin !== 'quick') return { ok: false, error: 'empty' };
  store.record(sid, { ev: 'QuickChat', prompt: prompt.slice(0, 2000) });
  runAndRecord(sid, quick.runPrint({ prompt, cwd: session.cwd, sessionId: sid, resume: true }));
  return { ok: true, sid };
}

function runAndRecord(sid, run) {
  run.done.then((res) => {
    store.record(sid, res.ok
      ? { ev: 'QuickChatDone', result: res.result.slice(0, 30000) }
      : { ev: 'QuickChatDone', error: String(res.error).slice(0, 600) });
  });
}

async function hatchPet(description) {
  description = String(description || '').trim().slice(0, 400);
  if (!description || hatching) return { ok: false, error: 'busy' };
  if (!quick.findClaude()) return { ok: false, error: 'claude-not-found' };
  hatching = true;
  lastHatch = null;
  broadcast();

  const sid = crypto.randomUUID();
  store.record(sid, { ev: 'QuickChat', hatch: true, prompt: description, cwd: paths.QUICK_DIR });
  const res = await quick.runPrint({
    prompt: petsLib.hatchPrompt(description), cwd: paths.QUICK_DIR, sessionId: sid, concise: false, timeoutMs: 5 * 60e3,
  }).done;
  store.record(sid, res.ok ? { ev: 'QuickChatDone', result: 'hatched' } : { ev: 'QuickChatDone', error: String(res.error).slice(0, 300) });

  try {
    if (!res.ok) throw new Error(res.error);
    const id = petsLib.saveCustom(petsLib.extractJson(res.result));
    pets = petsLib.loadAll();
    lastHatch = { ok: true, id };
    hatching = false;
    setConfig({ pet: id });
  } catch (err) {
    lastHatch = { ok: false, error: String(err.message).slice(0, 300) };
    hatching = false;
    broadcast();
  }
  return lastHatch;
}

function registerIpc() {
  ipcMain.on('ready', (e) => e.sender.send('state', publicState()));

  ipcMain.on('pet:ignore', (_e, ignore) => {
    petIgnoring = !!ignore;
    if (petWin && !drag) petWin.setIgnoreMouseEvents(petIgnoring);
  });
  ipcMain.on('pet:drag-start', () => {
    if (!petWin || drag) return;
    hidePanel();
    const cursor = screen.getCursorScreenPoint();
    const b = petWin.getBounds();
    drag = { dx: cursor.x - b.x, dy: cursor.y - b.y, width: b.width, height: b.height };
    drag.timer = setTimeout(endDrag, 120e3);
  });
  ipcMain.on('pet:drag-move', () => {
    if (!petWin || !drag) return;
    const p = screen.getCursorScreenPoint();
    petWin.setBounds({ x: p.x - drag.dx, y: p.y - drag.dy, width: drag.width, height: drag.height });
  });
  ipcMain.on('pet:drag-end', endDrag);
  ipcMain.on('pet:action', (_e, action, sid) => {
    if (action === 'chat') togglePanel('chat');
    else if (action === 'activity' || action === 'pet') togglePanel('home');
    else if (action === 'menu') popupPetMenu();
    else if (action === 'bubble') {
      const top = (sid && store.get(sanitize(sid))) || store.snapshot().overall.top;
      if (top) showPanel('thread', { sid: top.id }); else togglePanel('home');
    }
    if (!config.welcomed) setConfig({ welcomed: true });
  });

  ipcMain.on('panel:view', (_e, view) => {
    if (!PANEL_HEIGHTS[view]) return;
    panelView = view;
    if (panelWin && panelWin.isVisible()) positionPanel(view);
  });
  ipcMain.on('panel:close', hidePanel);
  ipcMain.handle('chat:send', (_e, payload) => startQuickChat(payload || {}));
  ipcMain.handle('chat:reply', (_e, payload) => replyToChat(payload || {}));
  ipcMain.on('thread:seen', (_e, sid) => store.record(sid, { ev: 'Seen' }));
  ipcMain.on('thread:dismiss', (_e, sid) => store.record(sid, { ev: 'Dismiss' }));
  ipcMain.on('threads:clear', () => {
    for (const s of store.snapshot().sessions) {
      if (s.status === 'ready' || s.status === 'blocked' || (s.status === 'idle' && !s.live)) store.record(s.id, { ev: 'Dismiss' });
    }
  });
  ipcMain.handle('thread:terminal', (_e, sid) => {
    const session = store.get(sid);
    if (!session) return false;
    hidePanel();
    return quick.resumeInTerminal({ cwd: session.cwd, sessionId: session.id });
  });
  ipcMain.on('thread:folder', (_e, sid) => {
    const session = store.get(sid);
    if (session && session.cwd && fs.existsSync(session.cwd)) shell.openPath(session.cwd);
  });
  ipcMain.on('claude-app:open', () => { hidePanel(); shell.openExternal('claude://'); });
  ipcMain.on('link:open', (_e, url) => {
    if (typeof url === 'string' && /^https?:\/\//i.test(url)) shell.openExternal(url);
  });

  ipcMain.on('config:set', (_e, patch) => setConfig(patch && typeof patch === 'object' ? patch : {}));
  ipcMain.handle('folder:pick', async () => {
    keepPanelOpen = true;
    try {
      const res = await dialog.showOpenDialog(panelWin, { properties: ['openDirectory'] });
      return res.canceled ? null : res.filePaths[0];
    } finally {
      keepPanelOpen = false;
      if (panelWin) panelWin.focus();
    }
  });

  ipcMain.handle('connect', () => {
    try { connection = connect.connect(); } catch (err) { connection = { ...connect.status(), error: err.message }; }
    broadcast();
    return connection;
  });
  ipcMain.handle('disconnect', () => {
    try { connection = connect.disconnect(); } catch (err) { connection = { ...connect.status(), error: err.message }; }
    broadcast();
    return connection;
  });

  ipcMain.on('pets:refresh', () => { pets = petsLib.loadAll(); trayKey = ''; broadcast(); });
  ipcMain.on('pets:delete', (_e, id) => {
    if (typeof id !== 'string') return;
    petsLib.deleteCustom(id);
    pets = petsLib.loadAll();
    if (config.pet === id) setConfig({ pet: 'ember' });
    resizePet();
    broadcast();
  });
  ipcMain.handle('pets:hatch', (_e, description) => hatchPet(description));

  ipcMain.on('pet:off', turnOff);

  ipcMain.on('ask:answer', (_e, sid, allow) => {
    sid = sanitize(sid);
    if (!asks.has(sid)) return;
    settleAsk(sid, allow ? 'allow' : 'deny');
    store.record(sid, { ev: 'Answered' });
  });
  ipcMain.on('history:clear', () => { store.clear(); alerted.clear(); });

  ipcMain.handle('pets:import', async () => {
    const file = await withDialog(() => dialog.showOpenDialog(panelWin, { properties: ['openFile'], filters: [{ name: 'Pet', extensions: ['json'] }] }));
    if (!file || file.canceled || !file.filePaths[0]) return { ok: false, cancelled: true };
    try {
      if (fs.statSync(file.filePaths[0]).size > 200e3) throw new Error('file too large');
      const id = petsLib.saveCustom(JSON.parse(fs.readFileSync(file.filePaths[0], 'utf8')));
      pets = petsLib.loadAll();
      setConfig({ pet: id });
      return { ok: true, id };
    } catch (err) {
      return { ok: false, error: String(err.message).slice(0, 200) };
    }
  });
  ipcMain.handle('pets:export', async (_e, id) => {
    const pet = pets.find((p) => p.id === id);
    if (!pet) return { ok: false };
    const slug = pet.name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'pet';
    const file = await withDialog(() => dialog.showSaveDialog(panelWin, { defaultPath: slug + '.json', filters: [{ name: 'Pet', extensions: ['json'] }] }));
    if (!file || file.canceled || !file.filePath) return { ok: false, cancelled: true };
    try {
      fs.writeFileSync(file.filePath, JSON.stringify(petsLib.portable(pet), null, 2) + '\n');
      return { ok: true };
    } catch (err) {
      return { ok: false, error: String(err.message).slice(0, 200) };
    }
  });
}

async function withDialog(open) {
  keepPanelOpen = true;
  try {
    return await open();
  } finally {
    keepPanelOpen = false;
    if (panelWin && !panelWin.isDestroyed()) panelWin.focus();
  }
}

function trackPointer() {
  const look = () => {
    let again = 60;
    if (petWin && petWin.isVisible() && !drag) {
      const p = screen.getCursorScreenPoint();
      const b = petWin.getBounds();
      const inside = p.x >= b.x && p.x < b.x + b.width && p.y >= b.y && p.y < b.y + b.height;
      if (inside) {
        again = 25;
        if (petIgnoring) petWin.webContents.send('pet:pointer', { x: p.x - b.x, y: p.y - b.y });
      } else if (pointerInside) {
        petIgnoring = true;
        petWin.setIgnoreMouseEvents(true);
        petWin.webContents.send('pet:pointer', null);
      }
      pointerInside = inside;
    }
    pointerTimer = setTimeout(look, again);
  };
  look();
}

function endDrag() {
  if (!drag) return;
  clearTimeout(drag.timer);
  drag = null;
  if (!petWin) return;
  const b = petWin.getBounds();
  config.anchor = { x: Math.round(b.x + b.width / 2), y: b.y + b.height };
  petWin.setBounds(petBounds());
  saveConfig();
}

app.on('second-instance', () => {});
app.on('window-all-closed', () => { });
let stopControl = () => {};
let stopPresence = () => {};
app.on('before-quit', () => { store.stop(); stopControl(); stopPresence(); clearTimeout(pointerTimer); globalShortcut.unregisterAll(); });

if (gotLock) app.whenReady().then(() => {
  fs.mkdirSync(paths.PET_HOME, { recursive: true });
  config.visible = true;
  saveConfig();
  pets = petsLib.loadAll();
  connection = connect.status();
  if (connection.connected || connection.partial) {
    try {
      connect.installHookFile();
      connect.installSkills();
      if (connection.outdated) connection = connect.connect();
    } catch {}
  }
  try { control.install(); } catch {}
  writeAppInfo();
  stopControl = control.listen(runCommand);
  trackPointer();

  registerIpc();
  createPetWindow();
  createPanelWindow();
  createTray();
  registerShortcut();

  store.on('change', () => {
    dropAnsweredAsks();
    try { announceChanges(); } catch {}
    broadcast();
  });
  store.start();
  setTimeout(() => { alertsArmed = true; }, 4000);
  stopPresence = presence.watchClaude({ sessionPids: () => store.livePids(), list: () => presence.listClaudePids(), onGone: () => app.quit() });
  screen.on('display-metrics-changed', resizePet);
  screen.on('display-removed', resizePet);
  setInterval(() => {
    const now = connect.status();
    if (now.connected !== connection.connected || now.partial !== connection.partial) { connection = now; broadcast(); }
  }, 15e3);
  broadcast();
});
