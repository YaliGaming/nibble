'use strict';
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const paths = require('./paths');

const EVENTS = {
  SessionStart: {},
  UserPromptSubmit: {},
  PreToolUse: { async: true },
  PostToolUse: { async: true },
  PostToolUseFailure: { async: true },
  PermissionRequest: { timeout: 600 },
  Notification: { async: true },
  ElicitationResult: { async: true },
  Stop: {},
  StopFailure: {},
  SessionEnd: {},
  UserPromptExpansion: { slashCommand: 'pet', timeout: 30 },
};

const forward = (file) => String(file).replace(/\\/g, '/');
const hookPath = () => forward(paths.HOOK_FILE);
const cliPath = () => forward(paths.CLI_FILE);

function hookEntry(opts) {
  const hook = {
    type: 'command',
    command: forward(paths.NODE_BIN),
    args: opts.slashCommand ? [cliPath(), 'toggle', '--hook'] : [hookPath()],
    timeout: opts.timeout || 5,
  };
  if (opts.async) hook.async = true;
  return hook;
}

function hookGroup(opts) {
  return opts.slashCommand ? { matcher: opts.slashCommand, hooks: [hookEntry(opts)] } : { hooks: [hookEntry(opts)] };
}

function isOurs(hook) {
  if (!hook) return false;
  const marks = ['.nibble-pet/hook.js', '.nibble-pet/pet.js', hookPath(), cliPath()];
  const mentions = (text) => typeof text === 'string' && marks.some((mark) => forward(text).includes(mark));
  return mentions(hook.command) || (Array.isArray(hook.args) && hook.args.some(mentions));
}

let nodeFound;
function nodeAvailable(refresh = false) {
  if (path.isAbsolute(paths.NODE_BIN)) return fs.existsSync(paths.NODE_BIN);
  if (nodeFound !== undefined && !refresh) return nodeFound;
  try {
    const cmd = process.platform === 'win32' ? 'where' : 'which';
    nodeFound = !!execFileSync(cmd, [paths.NODE_BIN], { encoding: 'utf8', windowsHide: true }).trim();
  } catch { nodeFound = false; }
  return nodeFound;
}

function readSettings() {
  let text;
  try { text = fs.readFileSync(paths.SETTINGS_FILE, 'utf8'); } catch (err) {
    if (err.code === 'ENOENT') return { settings: {}, existed: false };
    throw err;
  }
  if (!text.trim()) return { settings: {}, existed: true };
  const settings = JSON.parse(text.charCodeAt(0) === 0xFEFF ? text.slice(1) : text);
  if (!settings || typeof settings !== 'object' || Array.isArray(settings)) throw new Error('settings.json is not a JSON object');
  return { settings, existed: true };
}

function writeSettings(settings, existed) {
  fs.mkdirSync(path.dirname(paths.SETTINGS_FILE), { recursive: true });
  if (existed) {
    fs.mkdirSync(paths.BACKUP_DIR, { recursive: true });
    const stamp = new Date().toISOString().replace(/[:.]/g, '-');
    fs.copyFileSync(paths.SETTINGS_FILE, path.join(paths.BACKUP_DIR, `settings-${stamp}.json`));
  }
  const tmp = paths.SETTINGS_FILE + '.nibble.tmp';
  fs.writeFileSync(tmp, JSON.stringify(settings, null, 2) + '\n');
  fs.renameSync(tmp, paths.SETTINGS_FILE);
}

const ourEntries = (groups) => (Array.isArray(groups) ? groups : [])
  .filter((g) => g && Array.isArray(g.hooks) && g.hooks.some(isOurs));

function status() {
  const out = { connected: false, partial: false, outdated: false, node: nodeAvailable(), error: null, settingsFile: paths.SETTINGS_FILE };
  try {
    const { settings } = readSettings();
    const hooks = settings.hooks && typeof settings.hooks === 'object' ? settings.hooks : {};
    let have = 0;
    let current = 0;
    for (const [ev, opts] of Object.entries(EVENTS)) {
      const ours = ourEntries(hooks[ev]);
      if (!ours.length) continue;
      have++;
      if (ours.length === 1 && JSON.stringify(ours[0]) === JSON.stringify(hookGroup(opts))) current++;
    }
    const all = Object.keys(EVENTS).length;
    out.connected = have === all && fs.existsSync(paths.HOOK_FILE);
    out.partial = !out.connected && have > 0;
    out.outdated = have > 0 && current < all;
  } catch (err) {
    out.error = err.message;
  }
  return out;
}

function installHookFile() {
  fs.mkdirSync(paths.PET_HOME, { recursive: true });
  fs.copyFileSync(paths.HOOK_SOURCE, paths.HOOK_FILE);
  fs.copyFileSync(paths.CLI_SOURCE, paths.CLI_FILE);
}

function installSkills() {
  for (const [source, dir] of [[paths.SKILL_SOURCE, paths.SKILL_DIR], [paths.PET_SKILL_SOURCE, paths.PET_SKILL_DIR]]) {
    try {
      fs.mkdirSync(dir, { recursive: true });
      fs.copyFileSync(source, path.join(dir, 'SKILL.md'));
    } catch {}
  }
}

function removeSkills() {
  for (const dir of [paths.SKILL_DIR, paths.PET_SKILL_DIR]) {
    try { fs.rmSync(path.join(dir, 'SKILL.md'), { force: true }); fs.rmdirSync(dir); } catch {}
  }
}

const withoutOurs = (groups) => (Array.isArray(groups) ? groups : [])
  .map((g) => (g && Array.isArray(g.hooks) ? { ...g, hooks: g.hooks.filter((h) => !isOurs(h)) } : g))
  .filter((g) => !(g && Array.isArray(g.hooks) && g.hooks.length === 0));

function connect() {
  if (!nodeAvailable(true)) throw new Error('Node.js was not found. Reinstall the app, or install Node.js and try again.');
  const { settings, existed } = readSettings();
  installHookFile();

  if (!settings.hooks || typeof settings.hooks !== 'object' || Array.isArray(settings.hooks)) settings.hooks = {};
  for (const [ev, opts] of Object.entries(EVENTS)) {
    settings.hooks[ev] = [...withoutOurs(settings.hooks[ev]), hookGroup(opts)];
  }
  writeSettings(settings, existed);
  installSkills();
  return status();
}

function disconnect() {
  const { settings, existed } = readSettings();
  if (settings.hooks && typeof settings.hooks === 'object') {
    for (const ev of Object.keys(settings.hooks)) {
      if (!Array.isArray(settings.hooks[ev])) continue;
      const kept = withoutOurs(settings.hooks[ev]);
      if (kept.length) settings.hooks[ev] = kept; else delete settings.hooks[ev];
    }
    if (!Object.keys(settings.hooks).length) delete settings.hooks;
    if (existed) writeSettings(settings, existed);
  }
  removeSkills();
  return status();
}

module.exports = { EVENTS, status, connect, disconnect, installHookFile, installSkills, hookGroup };
