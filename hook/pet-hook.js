#!/usr/bin/env node
'use strict';
const fs = require('fs');
const os = require('os');
const path = require('path');

const t = Math.round(performance.timeOrigin);
const home = process.env.NIBBLE_HOME || path.join(os.homedir(), '.nibble-pet');

let config = {};
try { config = JSON.parse(fs.readFileSync(path.join(home, 'config.json'), 'utf8')) || {}; } catch {}
const keepText = config.storeText !== false;

const clip = (s, n) => (typeof s === 'string' ? (s.length > n ? s.slice(0, n) + '…' : s) : undefined);
const base = (p) => (typeof p === 'string' ? p.split(/[\\/]/).pop() : undefined);

function toolDetail(name, input) {
  if (!input || typeof input !== 'object') return undefined;
  switch (name) {
    case 'Bash':
    case 'PowerShell':
      return clip(input.description || input.command, 100);
    case 'Edit':
    case 'Write':
    case 'Read':
    case 'NotebookEdit':
      return clip(base(input.file_path || input.notebook_path), 80);
    case 'Grep':
    case 'Glob':
      return clip(input.pattern, 80);
    case 'WebSearch':
      return clip(input.query, 80);
    case 'WebFetch':
      try { return new URL(input.url).hostname; } catch { return undefined; }
    case 'Agent':
    case 'Task':
      return clip(input.description, 80);
    case 'AskUserQuestion': {
      const q = Array.isArray(input.questions) && input.questions[0];
      return clip(q && q.question, 200);
    }
    default:
      return undefined;
  }
}

function compact(e) {
  const text = (value) => (keepText ? value : undefined);
  const o = { t, ev: e.hook_event_name, sid: e.session_id, cwd: e.cwd };
  if (e.agent_id) o.agent = String(e.agent_id);
  if (process.env.CLAUDE_CODE_ENTRYPOINT) o.ep = process.env.CLAUDE_CODE_ENTRYPOINT;
  switch (e.hook_event_name) {
    case 'SessionStart':
      o.source = e.source;
      o.name = text(clip(e.session_title, 120));
      break;
    case 'UserPromptSubmit':
      o.prompt = text(clip(e.prompt, 400));
      o.name = text(clip(e.session_title, 120));
      break;
    case 'PreToolUse':
    case 'PostToolUse':
    case 'PostToolUseFailure':
    case 'PermissionRequest':
      o.tool = e.tool_name;
      o.detail = text(toolDetail(e.tool_name, e.tool_input));
      if (e.is_interrupt) o.interrupt = 1;
      break;
    case 'Notification':
      o.ntype = e.notification_type;
      o.msg = text(clip(e.message, 300));
      break;
    case 'Stop':
      o.msg = text(clip(e.last_assistant_message, 4000));
      break;
    case 'StopFailure':
      o.error = e.error;
      o.msg = text(clip(e.last_assistant_message || e.error_details, 600));
      break;
    case 'SessionEnd':
      o.reason = e.reason;
      break;
  }
  return o;
}

function pipePath() {
  if (process.platform !== 'win32') return path.join(home, 'pet.sock');
  return '\\\\.\\pipe\\nibble-' + require('crypto').createHash('sha1').update(home.toLowerCase()).digest('hex').slice(0, 12);
}

function wakePet(done) {
  if (config.visible !== true) return done();
  let settled = false;
  const socket = require('net').connect(pipePath());
  const finish = (running) => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    socket.destroy();
    if (!running) {
      try {
        require('child_process').spawn(process.execPath, [path.join(home, 'pet.js'), 'on'], { detached: true, stdio: 'ignore', windowsHide: true }).unref();
      } catch {}
    }
    done();
  };
  const timer = setTimeout(() => finish(true), 500);
  socket.on('connect', () => finish(true));
  socket.on('error', () => finish(false));
}

function petMayAnswer() {
  if (config.approvals === false) return false;
  if (process.env.NIBBLE_CHAT === '1') return true;
  if (process.env.CLAUDE_CODE_ENTRYPOINT === 'claude-desktop') return true;
  return config.approvalsEverywhere === true;
}

function askPet(e, done) {
  let settled = false;
  let text = '';
  const socket = require('net').connect(pipePath());
  const finish = (decision) => {
    if (settled) return;
    settled = true;
    clearTimeout(timer);
    socket.destroy();
    if (decision === 'allow' || decision === 'deny') {
      const answer = { behavior: decision };
      if (decision === 'deny') answer.message = 'Denied from Nibble.';
      process.stdout.write(JSON.stringify({ hookSpecificOutput: { hookEventName: 'PermissionRequest', decision: answer } }));
    }
    done();
  };
  const timer = setTimeout(() => finish(null), 9 * 60e3);
  socket.setEncoding('utf8');
  socket.on('connect', () => {
    const ask = { sid: String(e.session_id || ''), tool: e.tool_name, detail: toolDetail(e.tool_name, e.tool_input) };
    socket.write('ask ' + JSON.stringify(ask) + '\n');
  });
  socket.on('data', (chunk) => {
    text += chunk;
    if (!text.includes('\n')) return;
    try { finish(JSON.parse(text).decision); } catch { finish(null); }
  });
  socket.on('error', () => finish(null));
  socket.on('close', () => finish(null));
}

let raw = '';
const giveUp = setTimeout(() => process.exit(0), 4000);
process.stdin.setEncoding('utf8');
process.stdin.on('data', (c) => { raw += c; });
process.stdin.on('error', () => process.exit(0));
process.stdin.on('end', () => {
  clearTimeout(giveUp);
  const exit = () => process.exit(0);
  let e = null;
  try {
    e = JSON.parse(raw);
    const sid = String(e.session_id || '').replace(/[^A-Za-z0-9_-]/g, '');
    if (sid && e.hook_event_name) {
      const dir = path.join(home, 'events');
      fs.mkdirSync(dir, { recursive: true });
      fs.appendFileSync(path.join(dir, sid + '.jsonl'), JSON.stringify(compact(e)) + '\n');
    } else {
      e = null;
    }
  } catch { e = null; }

  try {
    const event = e && e.hook_event_name;
    if (event === 'SessionStart' || event === 'UserPromptSubmit') wakePet(exit);
    else if (event === 'PermissionRequest' && petMayAnswer()) askPet(e, exit);
    else exit();
  } catch { exit(); }
});
