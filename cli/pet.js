#!/usr/bin/env node
'use strict';
const fs = require('fs');
const os = require('os');
const net = require('net');
const path = require('path');
const crypto = require('crypto');
const { spawn, spawnSync } = require('child_process');

const home = process.env.NIBBLE_HOME || path.join(os.homedir(), '.nibble-pet');
const configFile = path.join(home, 'config.json');
const WORDS = { toggle: 'toggle', on: 'on', show: 'on', start: 'on', off: 'off', hide: 'off', chat: 'chat' };
const command = WORDS[process.argv[2]] || 'toggle';
const asHook = process.argv.includes('--hook');

function readJson(file) {
  try { return JSON.parse(fs.readFileSync(file, 'utf8')); } catch { return null; }
}

const app = readJson(path.join(home, 'app.json')) || {};
const TEXT = {
  on: 'Nibble is on.',
  off: 'Nibble is off. Type /pet again to bring it back.',
  started: 'Nibble is on. It will open and close together with Claude from now on.',
  chat: 'Quick Chat is open next to the pet.',
  notInstalled: 'Nibble has not been started on this computer yet. Open Nibble once from the Start menu.',
  failed: 'Nibble could not be reached.',
  ...(app.texts && typeof app.texts === 'object' ? app.texts : {}),
};

function say(key) {
  const line = String(TEXT[key]);
  if (!asHook) { console.log(line); return; }
  console.log(JSON.stringify({
    decision: 'block',
    reason: line,
    hookSpecificOutput: { hookEventName: 'UserPromptExpansion', suppressOriginalPrompt: true },
  }));
}

function pipePath() {
  if (process.platform !== 'win32') return path.join(home, 'pet.sock');
  return '\\\\.\\pipe\\nibble-' + crypto.createHash('sha1').update(home.toLowerCase()).digest('hex').slice(0, 12);
}

function request(word, timeout = 1500) {
  return new Promise((resolve) => {
    let text = '';
    let settled = false;
    const socket = net.connect(pipePath());
    const finish = (reply) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.destroy();
      resolve(reply);
    };
    const timer = setTimeout(() => finish(null), timeout);
    socket.setEncoding('utf8');
    socket.on('connect', () => socket.write(word + '\n'));
    socket.on('data', (chunk) => {
      text += chunk;
      if (!text.includes('\n')) return;
      try { finish(JSON.parse(text)); } catch { finish(null); }
    });
    socket.on('error', () => finish(null));
    socket.on('close', () => finish(null));
  });
}

function remember(on) {
  const config = readJson(configFile) || {};
  if (config.visible === on) return;
  try {
    fs.mkdirSync(home, { recursive: true });
    fs.writeFileSync(configFile, JSON.stringify({ ...config, visible: on }, null, 2));
  } catch {}
}

function start() {
  const args = Array.isArray(app.args) ? app.args.map(String) : [];
  if (!app.exe || !fs.existsSync(app.exe)) return false;
  remember(true);
  if (process.platform === 'win32') {
    const quote = (s) => "'" + String(s).replace(/'/g, "''") + "'";
    const line = args.map((a) => '"' + a + '"').join(' ');
    const script = `(New-Object -ComObject Shell.Application).ShellExecute(${quote(app.exe)}, ${quote(line)}, ${quote(path.dirname(app.exe))}, 'open', 1)`;
    const res = spawnSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true, timeout: 20000 });
    if (res.status === 0) return true;
  }
  try {
    spawn(app.exe, args, { detached: true, stdio: 'ignore' }).unref();
    return true;
  } catch {
    return false;
  }
}

async function run() {
  if (command === 'off') {
    await request('off');
    remember(false);
    return 'off';
  }
  if (command === 'toggle') {
    const reply = await request('off');
    if (reply && reply.ok) { remember(false); return 'off'; }
    return start() ? 'started' : 'notInstalled';
  }
  const reply = await request(command === 'chat' ? 'chat' : 'ping');
  if (reply && reply.ok) return command === 'chat' ? 'chat' : 'on';
  return start() ? 'started' : 'notInstalled';
}

run().then(say, () => say('failed')).finally(() => process.exit(0));
