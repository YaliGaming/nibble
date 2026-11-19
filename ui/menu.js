'use strict';
const api = window.petAPI;
const menu = document.getElementById('menu');

let state = null;
let t = PetI18n.translator({});
let expanded = false;
let lastHeight = 0;

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

const CHEVRON = '<svg class="chev" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="m9 6 6 6-6 6"/></svg>';
const TICK = '<svg class="tick" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12 5 5 9-10"/></svg>';

function entry(id, label, extra = '') {
  return `<button class="entry ${extra}" type="button" role="menuitem" data-id="${esc(id)}"><span class="label">${esc(label)}</span></button>`;
}

function render() {
  if (!state) return;
  const c = state.config;
  document.documentElement.dir = state.dir;
  document.documentElement.lang = c.lang;
  const shortcut = c.shortcutOk && c.shortcut ? c.shortcut.replace(/\+/g, ' + ') : '';
  const pets = state.pets
    .map((p) => `<button class="entry ${c.pet === p.id ? 'on' : ''}" type="button" role="menuitemradio" data-id="pet:${esc(p.id)}">${TICK}<span class="label">${esc(p.name)}</span></button>`)
    .join('')
    + `<button class="entry ${c.pet === 'mini' ? 'on' : ''}" type="button" role="menuitemradio" data-id="pet:mini">${TICK}<span class="label">${esc(t('settings.mini'))}</span></button>`;
  menu.innerHTML = [
    `<button class="entry" type="button" role="menuitem" data-id="chat"><span class="label">${esc(t('menu.chat'))}</span>${shortcut ? `<span class="hint">${esc(shortcut)}</span>` : ''}</button>`,
    entry('home', t('menu.activity')),
    '<div class="sep"></div>',
    `<button class="entry ${expanded ? 'open' : ''}" type="button" role="menuitem" aria-expanded="${expanded}" data-toggle="pets"><span class="label">${esc(t('menu.pet'))}</span>${CHEVRON}</button>`,
    expanded ? `<div class="sub">${pets}</div>` : '',
    entry('settings', t('menu.settings')),
    '<div class="sep"></div>',
    entry('off', t('menu.off'), 'danger'),
  ].join('');
  fit();
}

function fit() {
  const height = Math.ceil(menu.getBoundingClientRect().height) + 20;
  if (height && height !== lastHeight) {
    lastHeight = height;
    api.menuSize(height);
  }
}

menu.addEventListener('click', (e) => {
  const item = e.target.closest('.entry');
  if (!item) return;
  if (item.dataset.toggle) {
    expanded = !expanded;
    render();
    return;
  }
  api.menuPick(item.dataset.id);
});

document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') { api.menuClose(); return; }
  if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
  e.preventDefault();
  const items = [...menu.querySelectorAll('.entry')];
  const at = items.indexOf(document.activeElement);
  const next = e.key === 'ArrowDown' ? (at + 1) % items.length : (at <= 0 ? items.length - 1 : at - 1);
  items[next].focus();
});

api.onMenuShow(() => {
  expanded = false;
  lastHeight = 0;
  render();
  menu.querySelector('.entry')?.focus();
});

api.onState((s) => {
  state = s;
  t = PetI18n.translator(s.strings);
  render();
});
api.ready();
