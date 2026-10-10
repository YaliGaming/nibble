(function (root, factory) {
  if (typeof module === 'object' && module.exports) module.exports = factory();
  else root.PetI18n = factory();
})(typeof self !== 'undefined' ? self : this, function () {
  'use strict';

  function translator(strings) {
    const table = strings || {};
    return (key, vars) => {
      let s = typeof table[key] === 'string' ? table[key] : key;
      if (vars) for (const [k, v] of Object.entries(vars)) s = s.split(`{${k}}`).join(v);
      return s;
    };
  }

  const isolate = (text) => '⁨' + text + '⁩';
  const isolateLtr = (text) => '⁦' + text + '⁩';

  function describeActivity(t, activity, waiting) {
    if (!activity || !activity.tool) return '';
    const d = activity.detail || '';
    const tool = activity.tool;
    if (tool === 'AskUserQuestion') return d || t('act.question');
    if (tool === 'ExitPlanMode') return t('act.plan');
    const short = tool.startsWith('mcp__') ? tool.split('__').pop() : tool;
    if (waiting) return d ? `${short}: ${d}` : t('act.permission', { d: isolate(short) });
    if (!d) return tool === 'WebSearch' ? t('act.websearch') : t('act.tool', { d: isolate(short) });
    const piece = { d: isolate(d) };
    switch (tool) {
      case 'Bash': case 'PowerShell': return t('act.run', piece);
      case 'Edit': case 'Write': case 'NotebookEdit': return t('act.edit', piece);
      case 'Read': return t('act.read', piece);
      case 'Grep': case 'Glob': return t('act.search', piece);
      case 'WebFetch': return t('act.fetch', piece);
      case 'WebSearch': return t('act.websearch');
      case 'Agent': case 'Task': return t('act.agent', piece);
      default: return t('act.tool', { d: isolate(short) });
    }
  }

  return { translator, describeActivity, isolate, isolateLtr };
});
