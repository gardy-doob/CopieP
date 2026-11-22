// CopieP — interface du panneau.
'use strict';

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const listEl = $('list');
const searchEl = $('search');
const emptyEl = $('empty');

let items = [];
let shown = [];
let selectedId = null;
let busy = false;

// --- Utilitaires -----------------------------------------------------------

function icon(name) {
  const tpl = $('icons').content.querySelector(`[data-icon="${name}"]`);
  return tpl.cloneNode(true);
}

function escapeHtml(s) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function highlight(text, query) {
  const safe = escapeHtml(text);
  if (!query) return safe;
  const q = escapeHtml(query).replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return safe.replace(new RegExp(q, 'gi'), (m) => `<mark>${m}</mark>`);
}

function timeAgo(ms) {
  const s = Math.max(0, (Date.now() - ms) / 1000);
  if (s < 45) return 'à l’instant';
  if (s < 3600) return `il y a ${Math.round(s / 60)} min`;
  if (s < 86400) return `il y a ${Math.round(s / 3600)} h`;
  const d = Math.round(s / 86400);
  if (d === 1) return 'hier';
  if (d < 7) return `il y a ${d} jours`;
  return new Date(ms).toLocaleDateString('fr-FR', { day: 'numeric', month: 'short' });
}

let toastTimer;
function toast(msg) {
  const t = $('toast');
  t.textContent = msg;
  t.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { t.hidden = true; }, 2600);
}

async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    if (String(e) !== 'Déverrouillage annulé') toast(String(e));
    return undefined;
  }
}

// --- Rendu -----------------------------------------------------------------

function matches(it, q) {
  if (!q) return true;
  if (!it.preview) return false;
  return it.preview.toLowerCase().includes(q.toLowerCase());
}

function actionButton(name, title, onClick, extraClass = '') {
  const b = document.createElement('button');
  b.type = 'button';
  b.className = `icon-btn ${extraClass}`;
  b.title = title;
  b.appendChild(icon(name));
  b.addEventListener('click', (e) => { e.stopPropagation(); onClick(); });
  b.addEventListener('dblclick', (e) => e.stopPropagation());
  return b;
}

function renderCard(it, query) {
  const card = document.createElement('div');
  card.className = 'card';
  card.dataset.id = it.id;
  if (it.id === selectedId) card.classList.add('selected');

  if (it.protected && !it.revealed) {
    card.classList.add('is-locked');
    const kind = it.kind === 'image' ? 'Image' : `Texte · ${it.chars} caractère${it.chars > 1 ? 's' : ''}`;
    card.innerHTML = `
      <div class="locked">
        <button class="lock-ico" type="button" title="Afficher (Touch ID)"></button>
        <div><b>Élément protégé</b><span>${kind} · cadenas pour afficher</span></div>
      </div>`;
    const lockBtn = card.querySelector('.lock-ico');
    lockBtn.appendChild(icon('lock'));
    lockBtn.addEventListener('click', async (e) => { e.stopPropagation(); await call('unlock'); });
    lockBtn.addEventListener('dblclick', (e) => e.stopPropagation());
  } else if (it.kind === 'image') {
    const img = document.createElement('img');
    img.className = 'thumb';
    img.alt = '';
    if (it.thumb) img.src = it.thumb;
    card.appendChild(img);
    const meta = document.createElement('div');
    meta.className = 'meta';
    meta.textContent = `Image ${it.width} × ${it.height} · ${timeAgo(it.time)}`;
    card.appendChild(meta);
  } else {
    const text = document.createElement('div');
    text.className = 'text';
    text.innerHTML = highlight(it.preview || '', query);
    card.appendChild(text);
    const meta = document.createElement('div');
    meta.className = 'meta';
    meta.textContent = it.chars > 120 ? `${timeAgo(it.time)} · ${it.chars} caractères` : timeAgo(it.time);
    card.appendChild(meta);
  }

  // Marqueur discret quand on ne survole pas.
  if ((it.pinned || it.protected) && !card.classList.contains('is-locked')) {
    const badge = document.createElement('div');
    badge.className = 'badge';
    badge.appendChild(icon(it.protected ? 'lock' : 'pin'));
    card.appendChild(badge);
    card.classList.add('has-badge');
  }

  // Boutons au survol : épingler, épingler + protéger, supprimer.
  const actions = document.createElement('div');
  actions.className = 'actions';
  actions.appendChild(actionButton('pin',
    it.pinned ? 'Désépingler' : 'Épingler (garder dans l’historique)',
    () => togglePin(it), it.pinned ? 'on' : ''));
  actions.appendChild(actionButton('finger',
    it.protected ? 'Retirer la protection (Touch ID)' : 'Épingler et protéger par empreinte',
    () => toggleProtect(it), it.protected ? 'on' : ''));
  actions.appendChild(actionButton('trash', 'Supprimer', () => remove(it), 'danger'));
  card.appendChild(actions);

  card.addEventListener('click', () => select(it.id));
  card.addEventListener('dblclick', () => activate(it.id, 'paste'));
  return card;
}

function render() {
  const query = searchEl.value.trim();
  shown = items.filter((it) => matches(it, query));
  if (!shown.some((it) => it.id === selectedId)) selectedId = shown.length ? shown[0].id : null;

  listEl.replaceChildren();
  const pinned = shown.filter((it) => it.pinned);
  const recent = shown.filter((it) => !it.pinned);
  const addSection = (label) => {
    const s = document.createElement('div');
    s.className = 'section';
    s.textContent = label;
    listEl.appendChild(s);
  };
  if (pinned.length && recent.length) addSection('Épinglés');
  pinned.forEach((it) => listEl.appendChild(renderCard(it, query)));
  if (pinned.length && recent.length) addSection('Récents');
  recent.forEach((it) => listEl.appendChild(renderCard(it, query)));

  const empty = shown.length === 0;
  emptyEl.hidden = !empty;
  listEl.hidden = empty;
  if (empty && query) {
    $('empty-title').textContent = 'Aucun résultat';
    $('empty-text').textContent = 'Les éléments protégés ne sont cherchés qu’une fois déverrouillés.';
  } else {
    $('empty-title').textContent = 'Votre presse-papiers est vide';
    $('empty-text').textContent = 'Copiez du texte ou une image : l’historique apparaîtra ici.';
  }
}

function select(id, scroll = true) {
  selectedId = id;
  listEl.querySelectorAll('.card').forEach((c) => c.classList.toggle('selected', c.dataset.id === id));
  if (scroll) listEl.querySelector('.card.selected')?.scrollIntoView({ block: 'nearest' });
}

async function refresh() {
  items = (await call('list_items')) || [];
  render();
}

async function refreshStatus() {
  const st = await call('status');
  if (!st) return;
  document.body.classList.toggle('mac', st.mac);
  $('banner').hidden = !st.mac || st.accessibility;
}

// --- Actions ---------------------------------------------------------------

async function activate(id, mode) {
  if (!id || busy) return;
  busy = true;
  try {
    await call('activate_item', { id, mode });
  } finally {
    busy = false;
  }
}

async function togglePin(it) {
  await call('set_pinned', { id: it.id, pinned: !it.pinned });
}

async function toggleProtect(it) {
  await call('set_protected', { id: it.id, protect: !it.protected });
}

async function remove(it) {
  const idx = shown.findIndex((x) => x.id === it.id);
  const next = shown[idx + 1] || shown[idx - 1];
  if (next) selectedId = next.id;
  await call('delete_item', { id: it.id });
}

function selected() {
  return items.find((it) => it.id === selectedId);
}

function move(delta) {
  if (!shown.length) return;
  const i = Math.max(0, shown.findIndex((it) => it.id === selectedId));
  const n = Math.min(shown.length - 1, Math.max(0, i + delta));
  select(shown[n].id);
}

// « Tout effacer » demande une confirmation (deuxième clic).
let clearTimer;
$('clear').addEventListener('click', async () => {
  const b = $('clear');
  if (!b.classList.contains('confirm')) {
    b.classList.add('confirm');
    b.textContent = 'Confirmer ?';
    clearTimer = setTimeout(() => { b.classList.remove('confirm'); b.textContent = 'Tout effacer'; }, 3000);
    return;
  }
  clearTimeout(clearTimer);
  b.classList.remove('confirm');
  b.textContent = 'Tout effacer';
  await call('clear_history');
});

$('allow').addEventListener('click', () => call('open_accessibility_settings'));

searchEl.addEventListener('input', () => {
  selectedId = null;
  render();
});

document.addEventListener('keydown', (e) => {
  const cmd = e.metaKey || e.ctrlKey;
  const it = selected();
  switch (e.key) {
    case 'ArrowDown': e.preventDefault(); move(1); return;
    case 'ArrowUp': e.preventDefault(); move(-1); return;
    case 'PageDown': e.preventDefault(); move(5); return;
    case 'PageUp': e.preventDefault(); move(-5); return;
    case 'Enter': e.preventDefault(); if (it) activate(it.id, 'paste'); return;
    case 'Escape':
      e.preventDefault();
      if (searchEl.value) { searchEl.value = ''; render(); } else { call('hide'); }
      return;
    default: break;
  }
  if (!cmd) {
    if (document.activeElement !== searchEl && e.key.length === 1) searchEl.focus();
    return;
  }
  const key = e.key.toLowerCase();
  if (key === 'c' && it && !window.getSelection().toString()) { e.preventDefault(); activate(it.id, 'copy'); }
  else if (key === 'backspace' && it) { e.preventDefault(); remove(it); }
  else if (key === 'p' && it) { e.preventDefault(); togglePin(it); }
  else if (key === 'l' && it) { e.preventDefault(); toggleProtect(it); }
  else if (key === 'f') { e.preventDefault(); searchEl.focus(); searchEl.select(); }
  else if (/^[1-9]$/.test(key)) {
    const target = shown[Number(key) - 1];
    if (target) { e.preventDefault(); activate(target.id, 'paste'); }
  }
});

// Clic droit : pas de menu du navigateur.
document.addEventListener('contextmenu', (e) => e.preventDefault());

// --- Événements venant de l'application -------------------------------------

listen('history-changed', refresh);
listen('panel-shown', async () => {
  searchEl.value = '';
  selectedId = null;
  await refresh();
  refreshStatus();
  listEl.scrollTop = 0;
  searchEl.focus();
});

refresh();
refreshStatus();
