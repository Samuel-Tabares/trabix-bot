const state = {
  sessions: [],
  selectedSessionId: null,
  selectedMessages: [],
  selectedSnapshot: null,
  snapshotFetchedAt: null,
  timerRules: [],
  activeWorkspaceView: 'chat',
  activeDbTab: 'conversations',
  dbRows: {
    conversations: [],
    orders: [],
    order_items: [],
  },
  dbGeneratedAt: null,
  timersPopoverOpen: false,
  sendingCustomer: false,
  sendingAdvisor: false,
  refreshLocks: {},
};

const TIMER_FIELDS = [
  { key: 'advisor_response', field: 'advisor_response_seconds' },
  { key: 'receipt_upload', field: 'receipt_upload_seconds' },
  { key: 'advisor_stuck', field: 'advisor_stuck_seconds' },
  { key: 'relay_inactivity', field: 'relay_inactivity_seconds' },
  { key: 'conversation_reminder', field: 'conversation_reminder_seconds' },
  { key: 'conversation_reset', field: 'conversation_reset_seconds' },
];

const DB_TABLES = [
  {
    key: 'conversations',
    label: 'conversations',
    endpoint: '/simulator/api/db/conversations',
    columns: ['id', 'phone_number', 'state', 'customer_name', 'customer_phone', 'delivery_address'],
  },
  {
    key: 'orders',
    label: 'orders',
    endpoint: '/simulator/api/db/orders',
    columns: ['id', 'conversation_id', 'delivery_type', 'scheduled_date_text', 'scheduled_time_text', 'payment_method', 'receipt_media_id', 'delivery_cost', 'total_estimated', 'total_final', 'status'],
  },
  {
    key: 'order_items',
    label: 'order_items',
    endpoint: '/simulator/api/db/order-items',
    columns: ['id', 'order_id', 'flavor', 'has_liquor', 'quantity', 'unit_price', 'subtotal', 'created_at'],
  },
];

const bogotaFormatter = new Intl.DateTimeFormat('es-CO', {
  timeZone: 'America/Bogota',
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
  hour: '2-digit',
  minute: '2-digit',
  second: '2-digit',
  hour12: false,
});

const sessionList = document.getElementById('session-list');
const customerTranscript = document.getElementById('customer-transcript');
const advisorTranscript = document.getElementById('advisor-transcript');
const overrideGrid = document.getElementById('override-grid');
const dbTabs = document.getElementById('db-tabs');
const dbTableWrap = document.getElementById('db-table-wrap');
const dbMeta = document.getElementById('db-meta');
const showChatViewButton = document.getElementById('show-chat-view');
const showDbViewButton = document.getElementById('show-db-view');
const chatWorkspace = document.getElementById('chat-workspace');
const dbWorkspace = document.getElementById('db-workspace');
const timersToggleButton = document.getElementById('toggle-timers-popover');
const timersPopover = document.getElementById('timers-popover');
const timersPopoverList = document.getElementById('timers-popover-list');
const customerText = document.getElementById('customer-text');
const advisorText = document.getElementById('advisor-text');
const customerImageInput = document.getElementById('customer-image');
const pickCustomerImageButton = document.getElementById('pick-customer-image');
const sendCustomerButton = document.getElementById('send-customer');
const sendAdvisorButton = document.getElementById('send-advisor');
const customerImageStatus = document.getElementById('customer-image-status');
const sidebarTools = document.querySelector('.sidebar-tools');

async function fetchJson(url, options) {
  const response = await fetch(url, options);
  if (!response.ok) {
    throw new Error(`request failed: ${url}`);
  }
  if (response.status === 204) {
    return null;
  }
  return response.json();
}

async function withRefreshLock(key, fn) {
  if (state.refreshLocks[key]) {
    return;
  }
  state.refreshLocks[key] = true;
  try {
    await fn();
  } finally {
    state.refreshLocks[key] = false;
  }
}

function setWorkspaceView(view) {
  state.activeWorkspaceView = view === 'db' ? 'db' : 'chat';
  renderWorkspaceView();
}

function renderWorkspaceView() {
  const isDb = state.activeWorkspaceView === 'db';
  document.body.dataset.workspaceView = state.activeWorkspaceView;
  chatWorkspace.hidden = isDb;
  dbWorkspace.hidden = !isDb;
  showChatViewButton.classList.toggle('active', !isDb);
  showDbViewButton.classList.toggle('active', isDb);
}

function setTimersPopoverOpen(open) {
  state.timersPopoverOpen = Boolean(open);
  timersPopover.hidden = !state.timersPopoverOpen;
  timersToggleButton.classList.toggle('active', state.timersPopoverOpen);
  timersToggleButton.setAttribute('aria-expanded', String(state.timersPopoverOpen));
}

async function refreshSessionsList() {
  state.sessions = await fetchJson('/simulator/api/sessions');
  if (!state.selectedSessionId && state.sessions.length) {
    state.selectedSessionId = state.sessions[0].id;
  }
  if (state.selectedSessionId && !state.sessions.some((session) => session.id === state.selectedSessionId)) {
    state.selectedSessionId = state.sessions[0]?.id ?? null;
  }
  renderSessions();
  if (!state.selectedSessionId) {
    renderEmptySessionState();
  }
}

async function refreshSelectedSession(forceScroll = false) {
  if (!state.selectedSessionId) {
    renderEmptySessionState();
    return;
  }
  const [messages, snapshot] = await Promise.all([
    fetchJson(`/simulator/api/sessions/${state.selectedSessionId}/messages`),
    fetchJson(`/simulator/api/sessions/${state.selectedSessionId}/state`),
  ]);
  state.selectedMessages = messages;
  state.selectedSnapshot = snapshot;
  state.snapshotFetchedAt = Date.now();
  renderState(snapshot);
  renderTranscripts(forceScroll);
}

async function loadTimerRules() {
  const response = await fetchJson('/simulator/api/timer-overrides');
  state.timerRules = response.rules || [];
  renderTimerOverrides();
}

async function refreshDbTable(tabKey = state.activeDbTab) {
  const definition = DB_TABLES.find((table) => table.key === tabKey) || DB_TABLES[0];
  state.activeDbTab = definition.key;
  const response = await fetchJson(definition.endpoint);
  state.dbRows[definition.key] = response.rows || [];
  state.dbGeneratedAt = response.generated_at || null;
  renderDbPanel();
}

async function hardRefresh(forceScroll = false) {
  await refreshSessionsList();
  await refreshSelectedSession(forceScroll);
  await refreshDbTable(state.activeDbTab);
}

function renderEmptySessionState() {
  state.selectedMessages = [];
  state.selectedSnapshot = null;
  state.snapshotFetchedAt = null;
  customerTranscript.innerHTML = '<div class="empty-card empty-card-chat"><strong>Sin sesión activa</strong><span>Crea o selecciona una sesión para usar el panel del cliente.</span></div>';
  advisorTranscript.innerHTML = '<div class="empty-card empty-card-chat"><strong>Sin asesor asignado</strong><span>Selecciona una sesión para usar el panel del asesor.</span></div>';
  renderTimersPopover();
  syncCustomerComposer();
  syncAdvisorComposer();
}

function renderSessions() {
  sessionList.innerHTML = state.sessions.map((session) => `
    <div class="session-card ${session.id === state.selectedSessionId ? 'active' : ''}" data-session-id="${session.id}">
      <div class="session-card-top">
        <strong>${escapeHtml(session.profile_name || 'Sin nombre')}</strong>
        <span class="session-state-pill">${escapeHtml(session.state)}</span>
      </div>
      <div class="muted mono">${escapeHtml(session.customer_phone)}</div>
      <div class="session-address">${escapeHtml(session.delivery_address || 'Dirección pendiente')}</div>
      <div class="stamp">Actualizado ${escapeHtml(formatBogotaTimestamp(session.updated_at))}</div>
    </div>
  `).join('');

  for (const element of sessionList.querySelectorAll('[data-session-id]')) {
    element.addEventListener('click', async () => {
      const sessionId = Number(element.dataset.sessionId);
      if (Number.isNaN(sessionId) || sessionId === state.selectedSessionId) {
        return;
      }
      state.selectedSessionId = sessionId;
      renderSessions();
      await refreshSelectedSession(true);
    });
  }
}

function renderState(snapshot) {
  renderTimersPopover(snapshot);
  syncCustomerComposer();
  syncAdvisorComposer();
}

function renderTimersPopover(snapshot = state.selectedSnapshot) {
  if (!snapshot || !Array.isArray(snapshot.timers) || !snapshot.timers.length) {
    timersPopoverList.innerHTML = '<div class="empty-card"><strong>Sin timers</strong><span>No hay timers activos para esta sesión.</span></div>';
    return;
  }

  const elapsedClientSeconds = state.snapshotFetchedAt
    ? Math.max(0, Math.floor((Date.now() - state.snapshotFetchedAt) / 1000))
    : 0;

  timersPopoverList.innerHTML = snapshot.timers.map((timer) => {
    const remaining = Math.max(0, (timer.remaining_seconds || 0) - elapsedClientSeconds);
    const expired = timer.expired || remaining <= 0;
    return `
      <div class="timer-card ${expired ? 'expired' : ''}">
        <div class="timer-top">
          <strong>${escapeHtml(timer.label)}</strong>
          <span class="countdown">${expired ? 'Vencido' : escapeHtml(formatCountdown(remaining))}</span>
        </div>
        <div class="timer-meta">
          <span>${escapeHtml(timer.rule_key)} · ${escapeHtml(timer.phase)}</span>
          <span>${escapeHtml(timer.state)}</span>
          <span>ventana ${escapeHtml(String(timer.effective_seconds))} s</span>
          <span>vence ${escapeHtml(formatBogotaTimestamp(timer.expires_at))}</span>
        </div>
      </div>
    `;
  }).join('');
}

function renderTimerOverrides() {
  overrideGrid.innerHTML = state.timerRules.map((rule) => `
    <div class="override-card">
      <label for="override-${rule.key}">${escapeHtml(rule.label)}</label>
      <input
        id="override-${rule.key}"
        data-override-field="${lookupTimerField(rule.key)}"
        type="number"
        min="1"
        step="1"
        placeholder="${rule.default_seconds}"
        value="${rule.override_seconds ?? ''}">
      <div class="override-meta">
        base: ${escapeHtml(String(rule.default_seconds))} s
        <br>efectivo: ${escapeHtml(String(rule.effective_seconds))} s
      </div>
    </div>
  `).join('');

  for (const input of overrideGrid.querySelectorAll('[data-override-field]')) {
    input.addEventListener('change', persistTimerOverridesFromInputs);
  }
}

function renderTranscripts(forceScroll = false) {
  const shouldStickCustomer = forceScroll || shouldStickToBottom(customerTranscript);
  const shouldStickAdvisor = forceScroll || shouldStickToBottom(advisorTranscript);

  customerTranscript.innerHTML = state.selectedMessages
    .filter((message) => message.actor === 'customer' || message.audience === 'customer' || message.actor === 'system')
    .map((message) => renderMessage(message, 'customer'))
    .join('');

  advisorTranscript.innerHTML = state.selectedMessages
    .filter((message) => message.actor === 'advisor' || message.audience === 'advisor' || message.actor === 'system')
    .map((message) => renderMessage(message, 'advisor'))
    .join('');

  bindInteractiveActions();

  if (shouldStickCustomer) {
    customerTranscript.scrollTop = customerTranscript.scrollHeight;
  }
  if (shouldStickAdvisor) {
    advisorTranscript.scrollTop = advisorTranscript.scrollHeight;
  }
}

function renderMessage(message, pane) {
  const payload = message.payload || {};
  const extraImage = payload.media_url ? `<img class="img-preview" src="${payload.media_url}" alt="media">` : '';
  const actions = renderActions(message, pane);
  return `
    <div class="msg ${message.actor}">
      <div class="meta">
        <span>${escapeHtml(message.actor)} · ${escapeHtml(message.message_kind)}</span>
        <span class="stamp">${escapeHtml(formatBogotaTimestamp(message.created_at))}</span>
      </div>
      <div class="message-body">${escapeHtml(message.body || '')}</div>
      ${extraImage}
      ${actions}
    </div>
  `;
}

function renderActions(message, pane) {
  const payload = message.payload || {};
  if (message.message_kind === 'buttons' && Array.isArray(payload.buttons)) {
    return `<div class="actions">${payload.buttons.map((button) => `
      <button class="ghost action-button" data-pane="${pane}" data-kind="button" data-id="${button.reply.id}">${escapeHtml(button.reply.title)}</button>
    `).join('')}</div>`;
  }
  if (message.message_kind === 'list' && Array.isArray(payload.sections)) {
    const rows = payload.sections.flatMap((section) => section.rows || []);
    return `<div class="actions">${rows.map((row) => `
      <button class="ghost action-button" data-pane="${pane}" data-kind="list" data-id="${row.id}">${escapeHtml(row.title)}</button>
    `).join('')}</div>`;
  }
  return '';
}

function renderDbPanel() {
  dbTabs.innerHTML = DB_TABLES.map((table) => `
    <button class="db-tab ${table.key === state.activeDbTab ? 'active' : ''}" type="button" data-db-tab="${table.key}">
      ${escapeHtml(table.label)}
    </button>
  `).join('');

  for (const button of dbTabs.querySelectorAll('[data-db-tab]')) {
    button.addEventListener('click', async () => {
      const tab = button.dataset.dbTab;
      if (!tab || tab === state.activeDbTab) {
        return;
      }
      await refreshDbTable(tab);
    });
  }

  const definition = DB_TABLES.find((table) => table.key === state.activeDbTab) || DB_TABLES[0];
  const rows = state.dbRows[definition.key] || [];
  dbMeta.textContent = state.dbGeneratedAt
    ? `actualizado ${formatBogotaTimestamp(state.dbGeneratedAt)} · ${rows.length} filas`
    : 'sin datos cargados';

  if (!rows.length) {
    dbTableWrap.innerHTML = '<div class="empty-card"><strong>Sin filas</strong><span>No hay filas todavía para esta tabla.</span></div>';
    return;
  }

  dbTableWrap.innerHTML = `
    <table class="db-table">
      <thead>
        <tr>${definition.columns.map((column) => `<th>${escapeHtml(column)}</th>`).join('')}</tr>
      </thead>
      <tbody>
        ${rows.map((row) => `
          <tr>
            ${definition.columns.map((column) => `<td>${escapeHtml(formatDbCell(row[column]))}</td>`).join('')}
          </tr>
        `).join('')}
      </tbody>
    </table>
  `;
}

async function persistTimerOverridesFromInputs() {
  const payload = {};
  for (const field of TIMER_FIELDS) {
    payload[field.field] = null;
  }
  for (const input of overrideGrid.querySelectorAll('[data-override-field]')) {
    const value = input.value.trim();
    payload[input.dataset.overrideField] = value ? Number(value) : null;
  }
  const response = await fetchJson('/simulator/api/timer-overrides', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  state.timerRules = response.rules || [];
  renderTimerOverrides();
  if (state.selectedSnapshot) {
    renderTimersPopover();
  }
}

function bindInteractiveActions() {
  for (const button of document.querySelectorAll('.action-button')) {
    button.addEventListener('click', async () => {
      if (!state.selectedSessionId) {
        return;
      }
      const pane = button.dataset.pane;
      const kind = button.dataset.kind;
      const actor = pane === 'advisor' ? 'advisor' : 'customer';
      await fetch(`/simulator/api/sessions/${state.selectedSessionId}/${actor}/${kind}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: button.dataset.id }),
      });
      await hardRefresh(true);
    });
  }
}

async function postText(actor, body) {
  return fetchJson(`/simulator/api/sessions/${state.selectedSessionId}/${actor}/text`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ body }),
  });
}

async function postCustomerImage(file) {
  const formData = new FormData();
  formData.append('file', file);
  const response = await fetch(`/simulator/api/sessions/${state.selectedSessionId}/customer/image`, {
    method: 'POST',
    body: formData,
  });
  if (!response.ok) {
    throw new Error('customer_image_failed');
  }
}

function setCustomerStatus(message, tone = 'neutral') {
  customerImageStatus.textContent = message;
  customerImageStatus.classList.toggle('ready', tone === 'ready');
  customerImageStatus.classList.toggle('error', tone === 'error');
}

function syncCustomerComposer(preserveStatus = false) {
  const hasText = customerText.value.trim().length > 0;
  const hasImage = customerImageInput.files.length > 0;
  sendCustomerButton.disabled = state.sendingCustomer || !state.selectedSessionId || (!hasText && !hasImage);
  pickCustomerImageButton.disabled = state.sendingCustomer || !state.selectedSessionId;
  pickCustomerImageButton.classList.toggle('active', hasImage);

  if (preserveStatus) {
    return;
  }

  if (!state.selectedSessionId) {
    setCustomerStatus('Selecciona una sesión para escribir o adjuntar imágenes.');
    return;
  }

  setCustomerStatus(
    hasImage ? `Imagen lista: ${customerImageInput.files[0].name}` : 'Sin imagen seleccionada',
    hasImage ? 'ready' : 'neutral',
  );
}

function syncAdvisorComposer() {
  const hasText = advisorText.value.trim().length > 0;
  sendAdvisorButton.disabled = state.sendingAdvisor || !state.selectedSessionId || !hasText;
}

async function handleCustomerSend() {
  if (state.sendingCustomer) {
    return;
  }
  if (!state.selectedSessionId) {
    setCustomerStatus('Selecciona una sesión antes de enviar.', 'error');
    syncCustomerComposer(true);
    return;
  }

  const text = customerText.value.trim();
  const file = customerImageInput.files[0];
  if (!text && !file) {
    return;
  }

  state.sendingCustomer = true;
  syncCustomerComposer(true);

  let imageSent = false;
  let textSent = false;

  try {
    if (file) {
      setCustomerStatus(`Enviando imagen: ${file.name}`, 'ready');
      await postCustomerImage(file);
      imageSent = true;
      customerImageInput.value = '';
    }

    if (text) {
      setCustomerStatus(file ? 'Imagen enviada. Ahora va el mensaje...' : 'Enviando mensaje...');
      await postText('customer', text);
      textSent = true;
      customerText.value = '';
    }

    setCustomerStatus(imageSent && !textSent ? 'Imagen enviada.' : 'Envío completado.', 'ready');
    await hardRefresh(true);
  } catch (error) {
    if (!imageSent && file) {
      setCustomerStatus('No se pudo enviar la imagen. Intenta otra vez.', 'error');
    } else if (imageSent && text && !textSent) {
      setCustomerStatus('La imagen salió, pero el texto no. Reintenta el mensaje.', 'error');
    } else {
      setCustomerStatus('No se pudo enviar el mensaje. Intenta otra vez.', 'error');
    }
  } finally {
    state.sendingCustomer = false;
    syncCustomerComposer(true);
  }
}

async function handleAdvisorSend() {
  if (state.sendingAdvisor) {
    return;
  }
  if (!state.selectedSessionId) {
    return;
  }
  const body = advisorText.value.trim();
  if (!body) {
    return;
  }

  state.sendingAdvisor = true;
  syncAdvisorComposer();

  try {
    await postText('advisor', body);
    advisorText.value = '';
    await hardRefresh(true);
  } finally {
    state.sendingAdvisor = false;
    syncAdvisorComposer();
  }
}

function handleComposerKeydown(event, actor) {
  if (event.key !== 'Enter' || event.shiftKey) {
    return;
  }
  event.preventDefault();
  if (actor === 'advisor') {
    handleAdvisorSend().catch(() => {});
    return;
  }
  handleCustomerSend().catch(() => {});
}

function shouldStickToBottom(container) {
  return container.scrollHeight - container.scrollTop - container.clientHeight < 72;
}

function formatDbCell(value) {
  if (value === null || value === undefined || value === '') {
    return '(vacío)';
  }
  if (typeof value === 'object') {
    return JSON.stringify(value, null, 2);
  }
  return String(value);
}

function escapeHtml(value) {
  return String(value)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function lookupTimerField(key) {
  return TIMER_FIELDS.find((item) => item.key === key)?.field || '';
}

function formatBogotaTimestamp(value) {
  if (!value) {
    return '(vacío)';
  }
  return bogotaFormatter.format(new Date(value));
}

function formatCountdown(totalSeconds) {
  const seconds = Math.max(0, Number(totalSeconds) || 0);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const remainder = seconds % 60;
  const hh = hours ? `${String(hours).padStart(2, '0')}:` : '';
  return `${hh}${String(minutes).padStart(2, '0')}:${String(remainder).padStart(2, '0')}`;
}

document.getElementById('create-session-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const customerPhone = document.getElementById('new-phone').value.trim();
  const profileName = document.getElementById('new-name').value.trim();
  const session = await fetchJson('/simulator/api/sessions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ customer_phone: customerPhone, profile_name: profileName || null }),
  });
  document.getElementById('new-phone').value = '';
  document.getElementById('new-name').value = '';
  state.selectedSessionId = session.id;
  await hardRefresh(true);
});

document.getElementById('refresh-sessions').addEventListener('click', async () => {
  await hardRefresh();
});

document.getElementById('refresh-db').addEventListener('click', async () => {
  await refreshDbTable(state.activeDbTab);
});

document.getElementById('reset-overrides').addEventListener('click', async () => {
  const payload = {};
  for (const field of TIMER_FIELDS) {
    payload[field.field] = null;
  }
  const response = await fetchJson('/simulator/api/timer-overrides', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  state.timerRules = response.rules || [];
  renderTimerOverrides();
});

showChatViewButton.addEventListener('click', () => setWorkspaceView('chat'));
showDbViewButton.addEventListener('click', () => setWorkspaceView('db'));

timersToggleButton.addEventListener('click', () => {
  setTimersPopoverOpen(!state.timersPopoverOpen);
});

document.addEventListener('click', (event) => {
  if (!state.timersPopoverOpen) {
    return;
  }
  if (sidebarTools.contains(event.target)) {
    return;
  }
  setTimersPopoverOpen(false);
});

pickCustomerImageButton.addEventListener('click', () => customerImageInput.click());
customerImageInput.addEventListener('change', () => syncCustomerComposer());
customerImageInput.addEventListener('input', () => syncCustomerComposer());
customerText.addEventListener('input', () => syncCustomerComposer());
advisorText.addEventListener('input', () => syncAdvisorComposer());
customerText.addEventListener('keydown', (event) => handleComposerKeydown(event, 'customer'));
advisorText.addEventListener('keydown', (event) => handleComposerKeydown(event, 'advisor'));
sendCustomerButton.addEventListener('click', () => {
  handleCustomerSend().catch(() => {});
});
sendAdvisorButton.addEventListener('click', () => {
  handleAdvisorSend().catch(() => {});
});

renderWorkspaceView();
setTimersPopoverOpen(false);
syncCustomerComposer();
syncAdvisorComposer();

setInterval(() => renderTimersPopover(), 1000);
setInterval(() => {
  withRefreshLock('selected_session', async () => {
    await refreshSelectedSession();
  }).catch(() => {});
}, 2000);
setInterval(() => {
  withRefreshLock('sessions', async () => {
    await refreshSessionsList();
  }).catch(() => {});
  withRefreshLock('db', async () => {
    await refreshDbTable(state.activeDbTab);
  }).catch(() => {});
}, 5000);

Promise.all([
  loadTimerRules(),
  refreshSessionsList(),
  refreshDbTable(),
]).then(async () => {
  await refreshSelectedSession(true);
}).catch(() => {});
