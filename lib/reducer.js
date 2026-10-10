'use strict';

const ACTIVE = new Set(['running', 'needs_input']);
const NEEDS_INPUT_NOTIFS = new Set([
  'permission_prompt', 'elicitation_dialog', 'elicitation_url_dialog', 'agent_needs_input',
]);
const ASKING_TOOLS = new Set(['AskUserQuestion', 'ExitPlanMode']);
const HOOK_EVENTS = new Set([
  'SessionStart', 'UserPromptSubmit', 'PreToolUse', 'PostToolUse', 'PostToolUseFailure',
  'PermissionRequest', 'Notification', 'ElicitationResult', 'Stop', 'StopFailure', 'SessionEnd',
]);
const PRIORITY = ['needs_input', 'blocked', 'ready', 'running'];

const HOUR = 3600e3;

function blank(id) {
  return {
    id, cwd: '', name: '', regName: '', firstPrompt: '', lastPrompt: '',
    status: 'idle', statusSince: 0, activity: null, message: '', result: '',
    unread: false, ended: false, dismissed: false,
    origin: 'claude', surface: '', kind: '', pid: 0,
    hooked: false, lastHookT: 0, inRegistry: false, regStatus: '',
    pending: null, startedAt: 0, updatedAt: 0,
  };
}

function setStatus(s, status, t) {
  if (s.status !== status) { s.status = status; s.statusSince = t; }
}

const toolKey = (e) => `${e.tool || ''}|${e.detail || ''}`;

function apply(s, e) {
  const t = e.t || 0;
  if (!s.startedAt) s.startedAt = t;
  if (e.cwd) s.cwd = e.cwd;
  if (e.ep) s.surface = e.ep;
  if (e.name) s.name = e.name;

  if (HOOK_EVENTS.has(e.ev)) { s.hooked = true; s.lastHookT = t; s.updatedAt = t; }
  const agent = e.agent || '';

  switch (e.ev) {
    case 'SessionStart':
      s.ended = false;
      if ((e.source === 'startup' || e.source === 'clear') && ACTIVE.has(s.status)) setStatus(s, 'idle', t);
      break;

    case 'UserPromptSubmit':
      setStatus(s, 'running', t);
      if (e.prompt) { s.lastPrompt = e.prompt; if (!s.firstPrompt) s.firstPrompt = e.prompt; }
      s.activity = null; s.message = ''; s.result = ''; s.pending = null;
      s.unread = false; s.ended = false; s.dismissed = false;
      break;

    case 'PreToolUse':
      if (ASKING_TOOLS.has(e.tool)) {
        s.pending = { key: toolKey(e), agent };
        setStatus(s, 'needs_input', t);
        s.message = e.detail || '';
        s.activity = { tool: e.tool, detail: e.detail || '' };
        break;
      }
      if (s.pending && s.pending.agent === agent && s.pending.key !== toolKey(e)) s.pending = null;
      if (!s.pending) { setStatus(s, 'running', t); s.activity = { tool: e.tool, detail: e.detail || '' }; }
      break;

    case 'PermissionRequest':
      s.pending = { key: toolKey(e), agent };
      setStatus(s, 'needs_input', t);
      s.activity = { tool: e.tool, detail: e.detail || '' };
      s.message = '';
      break;

    case 'Notification':
      if (NEEDS_INPUT_NOTIFS.has(e.ntype)) {
        if (!s.pending) s.pending = { key: null, agent: '' };
        setStatus(s, 'needs_input', t);
        if (e.msg && !s.message) s.message = e.msg;
      }
      break;

    case 'ElicitationResult':
    case 'Answered':
      s.pending = null;
      if (s.status === 'needs_input') setStatus(s, 'running', t);
      break;

    case 'PostToolUse':
    case 'PostToolUseFailure':
      if (s.pending && (!s.pending.key || s.pending.key === toolKey(e))) s.pending = null;
      if (e.interrupt) {
        s.pending = null; s.activity = null;
        setStatus(s, 'idle', t);
      } else if (!s.pending) {
        if (s.status === 'needs_input') s.message = '';
        setStatus(s, 'running', t);
      }
      break;

    case 'Stop':
      s.pending = null; s.activity = null;
      setStatus(s, 'ready', t);
      s.statusSince = t;
      s.unread = true;
      s.message = e.msg || '';
      break;

    case 'StopFailure':
      s.pending = null; s.activity = null;
      setStatus(s, 'blocked', t);
      s.statusSince = t;
      s.unread = true;
      s.message = e.msg || e.error || '';
      break;

    case 'SessionEnd':
      s.ended = true; s.pending = null; s.activity = null;
      if (ACTIVE.has(s.status)) setStatus(s, 'idle', t);
      break;

    case 'QuickChat':
      s.origin = e.hatch ? 'hatch' : 'quick';
      setStatus(s, 'running', t);
      if (e.prompt) { s.lastPrompt = e.prompt; if (!s.firstPrompt) s.firstPrompt = e.prompt; }
      s.activity = null; s.message = ''; s.result = '';
      s.unread = false; s.ended = false; s.dismissed = false;
      s.updatedAt = t;
      break;

    case 'QuickChatDone':
      s.ended = true; s.pending = null; s.activity = null;
      if (e.error) {
        setStatus(s, 'blocked', t);
        s.message = e.error;
      } else {
        setStatus(s, 'ready', t);
        s.result = e.result || s.result;
        if (e.result) s.message = e.result;
      }
      s.statusSince = t;
      s.unread = true;
      s.updatedAt = t;
      break;

    case 'Seen':
      s.unread = false;
      if (s.status === 'ready' || s.status === 'blocked') setStatus(s, 'idle', t);
      break;

    case 'Dismiss':
      s.dismissed = true; s.unread = false;
      break;

    case 'Reg':
      s.pid = e.pid || 0; s.kind = e.kind || s.kind;
      if (e.rname) s.regName = e.rname;
      s.inRegistry = true; s.ended = false;
      break;

    case 'RegStatus': {
      const before = s.regStatus;
      s.regStatus = e.status || '';
      if (e.status === 'busy') {
        if (!s.hooked) {
          setStatus(s, 'running', t);
          s.pending = null; s.message = ''; s.unread = false; s.dismissed = false; s.updatedAt = t;
        } else if (s.status === 'needs_input' && before === 'waiting') {
          s.pending = null; s.message = '';
          setStatus(s, 'running', t);
        }
      } else if (e.status === 'waiting') {
        if (!s.hooked || s.status === 'running') {
          if (!s.pending) s.pending = { key: null, agent: '' };
          setStatus(s, 'needs_input', t);
          if (e.waitingFor && !s.message) s.message = e.waitingFor;
          if (!s.hooked) { s.dismissed = false; s.updatedAt = t; }
        }
      } else if (e.status === 'idle') {
        if (!s.hooked) {
          if (ACTIVE.has(s.status)) { setStatus(s, 'ready', t); s.pending = null; s.message = ''; s.unread = true; s.updatedAt = t; }
        } else if (s.status === 'running' && t > s.lastHookT + 1500) {
          setStatus(s, 'idle', t); s.activity = null;
        }
      }
      break;
    }

    case 'Gone':
      s.inRegistry = false; s.ended = true; s.pending = null; s.activity = null;
      if (ACTIVE.has(s.status)) setStatus(s, 'idle', t);
      break;
  }
  return s;
}

const clip = (str, n) => (str && str.length > n ? str.slice(0, n).trimEnd() + '…' : str || '');
const baseName = (p) => (p ? p.split(/[\\/]/).filter(Boolean).pop() || '' : '');

function view(s, now, opts = {}) {
  const readyTtl = (opts.readyTtlMin || 30) * 60e3;
  let status = s.status;
  let unread = s.unread;
  if (status === 'ready' && now - s.statusSince > readyTtl) { status = 'idle'; unread = false; }
  if (status === 'blocked' && now - s.statusSince > 4 * HOUR) { status = 'idle'; unread = false; }
  if (ACTIVE.has(status) && !s.inRegistry && now - s.updatedAt > 6 * HOUR) status = 'idle';

  const live = s.inRegistry && !s.ended;
  const quiet = now - s.updatedAt;
  const keep = live ? 12 * HOUR : (s.origin === 'quick' ? HOUR : HOUR / 4);
  const background = s.origin === 'claude' && (/^sdk/i.test(s.surface || '') || (!!s.kind && s.kind !== 'interactive'));
  const hidden = s.dismissed || s.origin === 'hatch' || (background && !opts.showBackground) || (status === 'idle' && quiet > keep);

  const firstLine = clip(((s.firstPrompt || '').split('\n').find((l) => l.trim()) || '').trim(), 70);
  const folder = baseName(s.cwd);
  const placeholderName = !!s.regName && !!folder && s.regName.length <= folder.length + 5 &&
    s.regName.toLowerCase().startsWith(folder.toLowerCase() + '-');
  const title = s.origin === 'quick'
    ? firstLine
    : (placeholderName ? '' : s.regName) || s.name || firstLine || s.regName;
  const noFolder = /[\\/]scratch-workspaces[\\/]/i.test(s.cwd || '') && /^scratch-/i.test(folder);
  return {
    id: s.id,
    title: title || (noFolder ? '' : folder) || '',
    cwd: s.cwd, folder: noFolder ? '' : folder, noFolder, background,
    status, unread, hidden, live,
    activity: s.activity, message: s.message, result: s.result, lastPrompt: s.lastPrompt,
    origin: s.origin, surface: s.surface, ended: s.ended, hooked: s.hooked,
    updatedAt: s.updatedAt, statusSince: s.statusSince,
    canReply: s.origin === 'quick' && !ACTIVE.has(status),
  };
}

function sortViews(views) {
  const rank = (v) => { const i = PRIORITY.indexOf(v.status); return i < 0 ? PRIORITY.length : i; };
  return views.slice().sort((a, b) => rank(a) - rank(b) || b.updatedAt - a.updatedAt);
}

function overall(views) {
  const visible = views.filter((v) => !v.hidden);
  const counts = { needs_input: 0, blocked: 0, ready: 0, running: 0, idle: 0 };
  for (const v of visible) counts[v.status] = (counts[v.status] || 0) + 1;
  const lastActivity = visible.reduce((m, v) => Math.max(m, v.updatedAt), 0);
  for (const status of PRIORITY) {
    const group = visible.filter((v) => v.status === status);
    if (group.length) {
      const top = group.reduce((a, b) => (b.updatedAt > a.updatedAt ? b : a));
      return { status, count: group.length, top, counts, lastActivity };
    }
  }
  return { status: 'idle', count: 0, top: null, counts, lastActivity };
}

module.exports = { blank, apply, view, sortViews, overall, ACTIVE, PRIORITY };
