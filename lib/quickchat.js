'use strict';
const fs = require('fs');
const os = require('os');
const path = require('path');
const crypto = require('crypto');
const { spawn, execFileSync } = require('child_process');
const paths = require('./paths');

const WIN = process.platform === 'win32';
const QUICK_SYSTEM_PROMPT =
  'This request comes from a small floating desktop companion window. Keep the answer short and direct unless detail is asked for.';

function which(name) {
  try {
    return execFileSync(WIN ? 'where' : 'which', [name], { encoding: 'utf8', windowsHide: true })
      .split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
  } catch { return []; }
}

let claudeBin;
function findClaude() {
  if (claudeBin !== undefined) return claudeBin;
  const found = which('claude');
  claudeBin = found.find((f) => /\.exe$/i.test(f)) || found[0] || null;
  if (!claudeBin) {
    const native = path.join(os.homedir(), '.local', 'bin', WIN ? 'claude.exe' : 'claude');
    if (fs.existsSync(native)) claudeBin = native;
  }
  return claudeBin;
}

const nodeAvailable = () => (path.isAbsolute(paths.NODE_BIN) ? fs.existsSync(paths.NODE_BIN) : which(paths.NODE_BIN).length > 0);

const isShim = (bin) => WIN && /\.(cmd|bat)$/i.test(bin);

function cleanEnv() {
  const env = { ...process.env };
  const inherited = ['CLAUDECODE', 'CLAUDE_CODE_ENTRYPOINT', 'CLAUDE_CODE_SESSION_ID', 'CLAUDE_CODE_HOST_SESSION_ID', 'CLAUDE_CODE_CHILD_SESSION'];
  if (!inherited.some((key) => env[key])) return env;
  for (const key of Object.keys(env)) {
    if (/^(CLAUDECODE$|CLAUDE_CODE_|CLAUDE_AGENT_SDK_|CLAUDE_PID$|CLAUDE_EFFORT$|CLAUDE_PREVIEW_)/.test(key)) delete env[key];
  }
  delete env.ANTHROPIC_BASE_URL;
  return env;
}

function existingDir(dir) {
  try { if (dir && fs.statSync(dir).isDirectory()) return dir; } catch {}
  return null;
}

function runPrint({ prompt, cwd, sessionId, resume = false, concise = true, timeoutMs = 20 * 60e3 }) {
  const bin = findClaude();
  if (!bin) return { child: null, done: Promise.resolve({ ok: false, error: 'claude-not-found' }) };

  const args = ['-p', '--output-format', 'json'];
  if (sessionId) args.push(resume ? '--resume' : '--session-id', sessionId);
  if (concise && !isShim(bin)) args.push('--append-system-prompt', QUICK_SYSTEM_PROMPT);

  const dir = existingDir(cwd) || paths.QUICK_DIR;
  fs.mkdirSync(dir, { recursive: true });

  let child;
  try {
    child = spawn(bin, args, { cwd: dir, env: { ...cleanEnv(), NIBBLE_CHAT: '1' }, windowsHide: true, shell: isShim(bin), stdio: ['pipe', 'pipe', 'pipe'] });
  } catch (err) {
    return { child: null, done: Promise.resolve({ ok: false, error: err.message }) };
  }

  const done = new Promise((resolve) => {
    let out = '';
    let err = '';
    const timer = setTimeout(() => child.kill(), timeoutMs);
    child.stdout.on('data', (d) => { out += d; });
    child.stderr.on('data', (d) => { err += d; });
    child.on('error', (e) => { clearTimeout(timer); resolve({ ok: false, error: e.message }); });
    child.on('close', (code) => {
      clearTimeout(timer);
      let parsed = null;
      try { parsed = JSON.parse(out); } catch {}
      if (Array.isArray(parsed)) parsed = parsed.filter((m) => m && m.type === 'result').pop() || null;
      if (parsed && parsed.type === 'result') {
        const text = typeof parsed.result === 'string' ? parsed.result : '';
        resolve(parsed.is_error ? { ok: false, error: text || parsed.subtype || 'error' } : { ok: true, result: text });
      } else if (code === 0 && out.trim()) {
        resolve({ ok: true, result: out.trim() });
      } else if (code === 0) {
        resolve({ ok: false, error: err.trim().slice(0, 600) || 'Claude Code finished without an answer.' });
      } else {
        resolve({ ok: false, error: (err.trim() || out.trim() || `claude exited with code ${code}`).slice(0, 600) });
      }
    });
    child.stdin.on('error', () => {});
    child.stdin.end(prompt);
  });
  return { child, done };
}

function openTerminal({ cwd, command, args = [] }) {
  const dir = existingDir(cwd) || os.homedir();
  const opts = { detached: true, stdio: 'ignore', env: cleanEnv() };
  if (WIN) {
    if (which('wt.exe').length) {
      const esc = (a) => String(a).replace(/;/g, '\\;');
      spawn('wt.exe', ['-d', esc(dir), esc(command), ...args.map(esc)], opts).unref();
    } else {
      spawn('cmd.exe', ['/c', 'start', '""', '/D', dir, command, ...args], opts).unref();
    }
    return true;
  }
  if (process.platform === 'darwin') {
    const quote = (a) => `'${String(a).replace(/'/g, `'\\''`)}'`;
    const line = `cd ${quote(dir)} && ${[command, ...args].map(quote).join(' ')}`;
    spawn('osascript', ['-e', `tell application "Terminal" to do script ${JSON.stringify(line)}`, '-e', 'tell application "Terminal" to activate'], opts).unref();
    return true;
  }
  for (const term of ['x-terminal-emulator', 'gnome-terminal', 'konsole', 'xterm']) {
    if (!which(term).length) continue;
    spawn(term, ['-e', command, ...args], { ...opts, cwd: dir }).unref();
    return true;
  }
  return false;
}

function resumeInTerminal({ cwd, sessionId }) {
  const bin = findClaude();
  if (!bin) return false;
  return openTerminal({ cwd, command: bin, args: ['--resume', sessionId] });
}

function chatInTerminal({ cwd, prompt }) {
  const bin = findClaude();
  if (!bin) return { ok: false, error: 'claude-not-found' };
  const dir = existingDir(cwd) || paths.QUICK_DIR;
  fs.mkdirSync(dir, { recursive: true });
  if (isShim(bin) || !nodeAvailable()) {
    return { ok: openTerminal({ cwd: dir, command: bin }), pasted: true };
  }
  fs.mkdirSync(paths.TMP_DIR, { recursive: true });
  const file = path.join(paths.TMP_DIR, crypto.randomUUID() + '.json');
  fs.writeFileSync(file, JSON.stringify({ bin, prompt }));
  return { ok: openTerminal({ cwd: dir, command: paths.NODE_BIN, args: [path.join(paths.APP_DIR, 'lib', 'launch-chat.js'), file] }) };
}

module.exports = { findClaude, runPrint, openTerminal, resumeInTerminal, chatInTerminal, cleanEnv };
