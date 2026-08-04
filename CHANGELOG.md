# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [1.19.1] - 2026-08-04

### Fixed
- **`degrade_agent_failure` dejaba al cliente en silencio cuando la falla ocurría en un turno de
  asesor.** El mensaje fijo `[agent].llm_failure_customer` solo se mandaba al cliente cuando
  `turn_actor == "customer"` — si el asesor intentaba destrabar un caso `needs_human` desde
  `crm-app` (`replyAsAdvisor`) y esa misma llamada al agente fallaba (timeout/5xx/saldo de
  Anthropic), el cliente no recibía nada, solo el asesor volvía a ver el aviso de error. Esto
  contradecía el comentario de la propia función ("el cliente NUNCA queda en silencio"). Caso real
  en producción: `573136356011` (2026-08-03) — el bot falló dos veces por falta de tokens, y el
  intento de respuesta manual del asesor ("tranqui") vía "Responder al bot" también falló sin que
  el cliente se enterara. `src/engine.rs`: el mensaje genérico ahora se manda al cliente sin
  importar qué turno disparó la falla. Test de regresión:
  `tests/agent_degradation.rs::advisor_turn_failure_still_notifies_the_customer`.

### Tests
- `src/ai/agent.rs`: dos tests nuevos para `confirm_order_bookkeeping` — uno cubre el camino feliz
  (dirección + zona resueltas → se emite `UpsertCustomerAddress`) y otro confirma que se omite el
  guardado si la zona nunca se resolvió. `customer_addresses` (máx. 4 direcciones, migración `015`)
  lleva 0 filas en producción desde que se desplegó (2026-08-02) — no por un bug encontrado, sino
  porque no hay un solo pedido confirmado desde esa fecha (el último es del 2026-07-22, sin probar
  aún el flujo end-to-end tras los últimos commits). Estos tests verifican que el mecanismo en sí
  funciona sin depender de un pedido real.

## [1.19.0] - 2026-08-02

### Added
- **Envío nacional (tercera forma de entrega, junto a Armenia y los 13 municipios).** Cualquier
  destino fuera de Armenia y fuera de la lista de pueblos cercanos ya no cae en el genérico
  "municipio desconocido": el agente lo reconoce como envío nacional (transportadora) y llama la
  nueva tool `set_delivery_national`.
  - `src/bot/delivery_zone.rs`: `MIN_UNITS_NATIONAL = 20` (mínimo sin excepción, coincide en valor
    con `MIN_UNITS_OUTSIDE_ARMENIA` y con el mínimo mayorista de sin licor, pero es una constante
    de negocio separada).
  - `src/ai/agent.rs`: `set_delivery_national` valida el mínimo y fija
    `pending_zone_kind = "national"`; el costo NO lo calcula el bot — se resigue el camino ya
    existente de domicilio manual (`message_advisor` + `set_manual_delivery_cost`), así que
    `finalize_checkout`, el estado `AskDeliveryCost` y la recuperación por timer de horario
    (`expire_business_hours_timer_with_source`) se reusan sin cambios.
  - El resultado de la tool trae, de forma imposible de omitir, el aviso obligatorio de que el
    envío nacional llega **descongelado** (el cliente lo congela al recibirlo) — promesa distinta
    a la de Armenia/municipios, donde llega listo para consumir. Reforzado también como regla dura
    en `SYSTEM_PROMPT`.
  - `select_saved_address` distingue direcciones guardadas con `zone_kind = "national"`: no
    reutiliza el costo de referencia (una transportadora puede cambiar tarifa), siempre vuelve a
    pedirlo al asesor.
  - Escala exclusivamente por `needs_human`/consola (`message_advisor` ya respeta
    `ADVISOR_WHATSAPP_ENABLED=false`), nunca por WhatsApp directo al asesor — compatible con la
    Fase 4 ya en producción.
  - **Confirmado (Samuel, 2026-08-02): la transportadora sí despacha con licor.** El canal nacional
    arranca con el mismo catálogo que el resto del retail (con licor), sin restricción de alcohol.

## [1.18.0] - 2026-08-02

### Changed
- **Fase 8: segundo listener para cerrar `/internal/*` a internet.** Antes `/internal/advisor/*` y
  `/internal/referral-codes*` colgaban del mismo `Router`/listener que `/webhook`, así que el
  dominio público de Railway los enrutaba igual (verificado: 401 desde afuera) y el único freno
  real era el `X-Internal-Token` compartido.
  - `src/routes/mod.rs`: `router()` se parte en `public_router()` (`/webhook`, `/privacy-policy`,
    `/terms-of-service`) e `internal_router()` (todo `/internal/*`).
  - `src/main.rs`: arranca los dos routers en dos `TcpListener`/`axum::serve` distintos —
    `public_router()` en `PORT` (el que Railway expone), `internal_router()` en `INTERNAL_PORT`
    (nuevo, default `8081`) — corriendo con `tokio::try_join!`. Railway no le asigna dominio
    público al segundo puerto, así que solo queda alcanzable por la red privada
    (`trabix-bot.railway.internal:8081`).
  - `src/config.rs`: nuevo campo `internal_port` (env `INTERNAL_PORT`, default `8081`, mismo
    criterio de fallback-a-default que `PORT`).
  - **Requiere acción al desplegar:** actualizar `TRABIX_BOT_INTERNAL_URL` en `crm-app` de
    `http://trabix-bot.railway.internal:8080` a `http://trabix-bot.railway.internal:8081` — si no,
    el envío saliente empieza a fallar con `not_connected`. Detalle en `general_info/runbook.md`.

## [1.17.0] - 2026-08-02

### Changed
- **Fase 6: códigos de referido en base de datos, reemplaza `config/referrals.toml`.** Cambiar un
  código o activar un boost ya no exige desplegar el bot. Nueva tabla `referral_codes` (migración
  `016`, sembrada con los 5 códigos legacy) con `active` y `boost_until` (ventana temporal real de
  7 días, no un flag fijo — expira sola en vez de vivir permanente como el `boost_codes` del TOML).
  - `src/referrals.rs`: `ReferralRegistry` pasa de cargarse una vez desde TOML
    (`OnceLock` write-once) a cargarse desde la DB y cachearse en un
    `OnceLock<RwLock<Arc<ReferralRegistry>>>`, refrescado por un `tokio::spawn` en background cada
    30s (`main.rs`) más un refresco inmediato tras cada escritura. Los call sites existentes
    (`ai/tools.rs`, `bot/states/checkout.rs`) no cambiaron: `referral_registry()` sigue siendo
    síncrona.
  - 3 endpoints internos nuevos en `src/routes/internal.rs` (mismo `X-Internal-Token` que
    `/internal/advisor/send`): `POST /internal/referral-codes` (crear), `PATCH
    /internal/referral-codes/:code` (activar/desactivar), `POST
    /internal/referral-codes/:code/boost` (activar boost de 7 días). El bot sigue siendo el único
    escritor de la tabla; `crm-app` la lee directo por Postgres (solo lectura, como
    `customers`/`orders`) y escribe solo por estos endpoints.
  - `config/referrals.toml` eliminado.

## [1.16.0] - 2026-08-02

### Added
- **Fase 5: direcciones guardadas del cliente (recompra).** Hasta 4 direcciones reales por
  cliente en la tabla nueva `customer_addresses` (migración `015`, append-only), cada una con la
  zona ya resuelta (`zone_kind`/`zone_value`, reconstruible con `bot::delivery_zone`) y un
  `last_delivery_cost_cop` de referencia — nunca usado para cobrar: el costo real de un checkout
  siempre sale de una tool call en vivo (`set_delivery_zone_armenia`/`set_delivery_nearby_town`),
  igual que antes. `customers.delivery_address_last` sigue funcionando sin cambios.
  - Dos tools nuevas del agente: `list_saved_addresses` (lista hasta 4, sin tocar la DB — el
    prefetch lo hace `engine.rs` antes de cada turno) y `select_saved_address(address_id)`
    (reutiliza una guardada; el modelo igual debe fijar el costo real después).
  - Al confirmar un pedido (transferencia o contraentrega), `confirm_order_bookkeeping` guarda o
    refresca la dirección en `customer_addresses` (`db::queries::upsert_customer_address`): si el
    cliente ya tiene 4 y la nueva es distinta, se descarta la de `created_at` más antiguo sin
    preguntar (decisión de producto).
  - `ArmeniaZone::storage_key`/`from_storage_key` y `NearbyTown.key` nuevos en
    `bot::delivery_zone` para persistir/reconstruir la zona sin duplicar su lógica en otro lado.

## [1.15.0] - 2026-08-02

### Added
- **Fase 2: toma de control humana con auto-devolución.** El bot le seguía respondiendo al cliente
  aunque un asesor ya estuviera escribiéndole desde `crm-app` — dos voces en la misma conversación.
  Ahora, cada `POST /internal/advisor/send` (texto libre del asesor al cliente) marca
  `conversations.human_takeover_until = now + ADVISOR_TAKEOVER_HOURS` (env nueva, default `6h`,
  ventana deslizante — se reemplaza en cada envío, no se acumula). Mientras esa columna sigue en el
  futuro:
  - `engine::process_customer_input` no llama al agente para ese cliente (el mensaje entrante sigue
    quedando en `message_events`, visible en la bandeja, pero el bot no lo procesa ni agenda timers
    nuevos).
  - Los 4 `expire_*_with_source` de `bot::timers` (asesor, relay, inactividad genérica, reapertura
    de horario) y la reconciliación de timers vencidos al boot se vuelven no-op mientras dure la
    pausa.
  - Nuevo `POST /internal/advisor/release` devuelve la conversación al bot antes de que venza la
    ventana (botón "Devolver al bot" en `crm-app`).
  - **`POST /internal/advisor/reply` deliberadamente NO dispara la pausa** — ese endpoint existe
    para que el bot siga el checkout automático después de que el asesor destraba una pregunta
    puntual (`confirm_advisor_availability`, `set_manual_delivery_cost`); pausarlo ahí rompería el
    propósito del endpoint. Contrato completo en `docs/internal_advisor_send.md`.
  - Migración `014_add_conversation_human_takeover.sql` (columna `human_takeover_until`, append-only).

## [1.14.0] - 2026-08-02

### Changed
- **Fase 1: se elimina la pregunta bloqueante de disponibilidad inmediata.** Antes, cualquier
  pedido `immediate` (dentro o fuera de horario) detenía el flujo esperando que el asesor
  contestara `confirm_advisor_availability` ("¿puedes entregar ya?"), incluso cuando el domicilio
  ya se conocía. Ahora:
  - **Dentro de horario (8:00–23:00)** con domicilio ya conocido: `finalize_checkout` autoacepta
    solo, igual que ya hacían los programados (`auto_accept_scheduled_order` se generalizó a
    `auto_accept_order`, reusada por ambos casos). Si el domicilio no se conoce (municipio/zona
    fuera de lista), se le pide el VALOR al asesor con `message_advisor` — nunca disponibilidad —
    y `set_manual_delivery_cost` autoacepta apenas responde.
  - **Fuera de horario**: ya no se rechaza (`checkout_precondition_error` y `set_delivery_immediate`
    dejaron de bloquear `immediate` cerrado). El pedido completo queda guardado
    (`pending_advisor`), el asesor recibe un aviso informativo (no una pregunta — deja el caso
    visible como `needs_human` en `crm-app` sin depender de que conteste nada) y el cliente recibe
    un mensaje claro de que se confirma automáticamente al abrir. Nuevo estado
    `ConversationState::WaitBusinessHours` (deliberadamente distinto de `AskDeliveryCost`, que
    hereda un timeout de 10-30 min por inactividad que habría reseteado el pedido antes de que
    abriéramos) se resuelve solo vía el sweep de 60s ya existente
    (`timers::expire_business_hours_timer_with_source`, nuevo `TimerType::BusinessHoursReopen`),
    sin necesidad de calcular duraciones ni armar un timer en vivo.
  - `confirm_advisor_availability` quedó sin call-sites reales y se borró por completo (tool
    definition, dispatch, función y tests) — la vía de "el asesor rechaza el pedido" que ofrecía se
    reemplaza por `cancel_order` o intervención directa del asesor.
  - Fuera de alcance a propósito: destinos desconocidos (envío nacional). Siempre son pedidos
    `scheduled`, ya auto-aceptan al conocerse el costo y ya quedan `needs_human` en `crm-app` sin
    cambios — es el único paso bloqueante que sigue en pie, por decisión de negocio.

## [1.13.0] - 2026-08-02

### Added
- **Backfill de `customers`** (migración `013`): la tabla se creó el 2026-07-15 y solo capturaba
  desde ahí, pero los pedidos existen desde marzo — de los 17 teléfonos con pedido, solo **4** tenían
  fila. Los otros 13 eran invisibles en la consola: no salían en contactos, no contaban para
  recompra, no existían para ninguna métrica. Se reconstruye lo derivable de datos ya guardados
  (teléfono, nombre, última dirección, primer/último contacto, y gasto/unidades **solo de pedidos
  `confirmed`). `customer_username` y `ctwa_clid` quedan nulos a propósito: nunca se capturaron y
  rellenarlos sería inventar datos. Idempotente (`ON CONFLICT DO NOTHING`), nunca pisa una fila
  existente. Solo entran conversaciones con evidencia de ser cliente — así queda afuera la fila que
  el bot crea para el número del **asesor**, que no es un cliente.

### Changed
- **El corte de Fase 4 ahora aplica en las dos direcciones.** Antes `ADVISOR_WHATSAPP_ENABLED=false`
  solo silenciaba la salida hacia el asesor, pero el webhook seguía enrutando cualquier mensaje
  entrante de ese número como "asesor". O sea: el número quedaba secuestrado como canal de control y
  ni siquiera podía probar el bot como cliente. Ahora, con el canal apagado, ese número vuelve a ser
  uno cualquiera y el bot le habla como cliente.

## [1.12.0] - 2026-08-02

### Fixed
- **El bot perdía mensajes de clientes en silencio** (`src/whatsapp/types.rs`): `ContactProfile.name`
  era obligatorio, pero Meta manda `"profile": {}` sin `name` cuando el usuario no tiene nombre de
  perfil configurado. Serde rechazaba el **payload completo** (`missing field 'name' at line 1
  column 260`), el webhook devolvía error y el mensaje nunca llegaba al motor: el cliente no recibía
  respuesta y no quedaba ni rastro en `message_events`. Ahora el campo es `Option<String>`.
  Detectado en producción el 2026-08-02 con clientes reales escribiendo. Test de regresión con el
  payload exacto que fallaba.

### Added
- **Fase 4 — corte del canal directo bot→asesor, detrás de un flag.**
  `ADVISOR_WHATSAPP_ENABLED` (default **`true`**, o sea sin cambio de comportamiento al desplegar).
  En `false`, todo lo dirigido al `ADVISOR_PHONE` deja de salir a Meta pero **se sigue escribiendo
  en `message_events`** con `channel='advisor'` — el asesor deja de recibir WhatsApp y pasa a
  atender desde `crm-app`, sin que el bot pierda la traza ni cambie su lógica. El corte se hace en
  `send_via_transport` (`src/engine.rs`), el único punto por el que pasa todo lo saliente, y
  a propósito **después** de `log_outbound_event`. Un valor no reconocido cae al default: quedarse
  sin canal humano por un typo sería peor que ignorar la variable.
- **`POST /internal/advisor/reply`** (`src/routes/internal.rs`): la respuesta del asesor **hacia el
  bot**, no hacia el cliente. Inyecta el mensaje en el turno de agente del asesor igual que si
  hubiera contestado por WhatsApp. Es lo que hace que sigan funcionando
  `confirm_advisor_availability` (¿puedes entregar ya?) y `set_manual_delivery_cost` (¿cuánto vale
  el domicilio?), que son **pasos bloqueantes del flujo de pedido**. Sin esto, apagar el WhatsApp
  del asesor dejaría esos pedidos colgados para siempre: `/internal/advisor/send` manda texto crudo
  al cliente y se salta el agente, así que no sirve para contestarle al bot.
- `engine::process_advisor_turn_for_case`: el turno del asesor sobre un caso concreto, extraído de
  `process_advisor_input`. Las dos vías (WhatsApp y `crm-app`) solo difieren en cómo se resuelve el
  caso — por botón/reply, o explícito en el request.

## [1.11.0] - 2026-08-01

### Added
- **`POST /internal/advisor/send`** (`src/routes/internal.rs`): endpoint interno para que `crm-app`
  mande WhatsApp al cliente **sin volverse un segundo escritor** sobre la conversación — el bot
  sigue siendo el único dueño de la sesión de Meta y el único que traza. Autenticado con el header
  `X-Internal-Token` contra la nueva variable opcional `INTERNAL_API_TOKEN` (comparación de tiempo
  constante); si la variable no está configurada el endpoint responde `503`, o sea queda
  deshabilitado y nunca abierto. Toma el mismo lock de conversación que el motor de agente, exige
  que la conversación exista (guard contra mandar a un número arbitrario), y escribe la traza en
  `message_events` con `channel='client'`, `actor='advisor'` y
  `payload={"source":"crm-app","sent_by":...}`. Los errores salen como `{code, message}` con
  códigos alineados a `SendError` de `crm-app`; en particular distingue `window_closed` (Meta
  131047/470, ventana de 24h vencida → hace falta plantilla) de `meta_error`/`meta_unavailable`.
  La traza y el `last_message_at` son best-effort: si fallan, la respuesta sigue siendo `200`
  porque el mensaje ya salió y reintentar duplicaría el envío al cliente.
  Contrato completo: `docs/internal_advisor_send.md`.

### Notes
- No se tocó `src/bot/states/advisor.rs` ni `relay.rs`: el flujo viejo (bot → WhatsApp del asesor)
  sigue intacto en producción. Este endpoint es un camino nuevo en paralelo, prerrequisito del
  envío saliente de `crm-app`.

## [1.10.0] - 2026-07-31

### Added
- **CTWA click ID + Conversions API de Meta (plumbing completo, falta solo credenciales)**:
  `ctwa_clid` se captura del objeto `referral` en el primer mensaje de un cliente que llega por un
  anuncio click-to-WhatsApp (`src/whatsapp/types.rs`, `src/routes/webhook.rs`) y se guarda una sola
  vez en `customers.ctwa_clid` (migración `011`, nunca se sobreescribe). Nuevo `src/capi.rs`
  (`CapiClient`) reporta un evento `Purchase` a la Conversions API de Meta en el momento real de
  confirmación de compra (`BotAction::UpdateCustomerAndAnalytics` en `src/engine.rs`, solo cuando el
  delta de venta es positivo) — corre en background (`tokio::spawn`), nunca bloquea ni demora la
  confirmación del pedido, y falla en silencio si `WABA_ID`/`META_CAPI_DATASET_ID`/
  `META_CAPI_ACCESS_TOKEN` no están configuradas. Ver `docs/PENDIENTE_capi_meta.md`.
- **`send_quick_replies`/`send_options_list`** como tools del agente de IA (`src/ai/agent.rs`),
  sobre los `BotAction::SendButtons`/`SendList` que ya existían pero nunca llamaba el motor de
  agente. Hasta 3 botones o 10 filas de lista (límites duros de WhatsApp), con validación de
  longitud de título/descripción.

### Changed
- **Se eliminó el toggle `BOT_ENGINE`**: el bot corre el motor de agente siempre;
  `ANTHROPIC_API_KEY` pasa a ser obligatoria para arrancar. El toggle ya no cambiaba ningún
  comportamiento observable en producción — se elimina la rama muerta, no una feature.

### Removed
- `reminder_actions()` (`src/bot/inactivity.rs`) y sus helpers de botones huérfanos en
  `src/bot/timers.rs` — código muerto real: solo se llamaban desde la rama determinista del timer
  de inactividad, inalcanzable desde que el bot corre agente siempre.
- Migración `012`: `DROP TABLE` de las tablas huérfanas del simulador que creó `005` (removido en
  v1.8.0).

## [1.9.0] - 2026-07-31

### Added
- **Domicilio gratis en Armenia (6-19 unidades) + detal sin mínimo en pueblos aledaños**:
  `delivery_zone::armenia_delivery_cost` cobra $0 de domicilio en Armenia para pedidos de 6 a 19
  unidades (tarifa de zona por debajo de 6, precio mayorista + domicilio cobrado desde 20).
  `units_until_free_delivery` calcula cuántas unidades faltan para calificar, usado para el mensaje
  de mayor impacto en ticket promedio ("agrega N unidades y el domicilio te sale GRATIS"). Los
  pueblos cercanos ahora se dividen en `TownGroup::Aledano` (Calarcá, El Caimo, Circasia,
  Montenegro, La Tebaida, Pueblo Tapao, Barcelona — detal completo, **sin mínimo** de unidades) y
  `TownGroup::Lejano` (Quimbaya, Salento, Filandia, Buenavista, Pijao, Córdoba, Génova — mantiene el
  mínimo de 20 unidades por costo de oportunidad operativo). El domicilio gratis es exclusivo de
  Armenia: fuera de la ciudad siempre se cobra tarifa, sin excepción. `set_delivery_zone_armenia` y
  `set_delivery_nearby_town` (`src/ai/agent.rs`) quedan wireados a la nueva lógica; el system prompt
  del agente se actualizó para reflejar las reglas. Ver `docs/PENDIENTE_domicilio_gratis.md`.
- **Prompt caching en el motor de agente**: el `system` de la Messages API ahora se envía como
  dos bloques — el system prompt estático (`SYSTEM_PROMPT`) marcado con `cache_control:
  {"type": "ephemeral"}`, y el bloque dinámico "ESTADO ACTUAL DEL CASO" (hora, pedido, quién
  escribe) sin marca, después del estático. El breakpoint en el último bloque de `system` cachea
  también las definiciones de tools (van antes en el render de la API). `AnthropicClient::send_message`
  ahora recibe `static_system`/`dynamic_system` por separado en vez de un solo `&str`, y loguea
  `usage.cache_read_input_tokens`/`cache_creation_input_tokens` en cada llamada (`tracing::debug!`)
  para medir el ahorro real. Ver `docs/PENDIENTE_prompt_caching.md`.
- Nuevo sabor **Smirnoff de tamarindo** (con licor) en el catálogo (`config/messages.toml`,
  `LIQUOR_FLAVOR_IDS`, `REQUIRED_LIQUOR_FLAVOR_IDS`).
- **Regla sin licor agotado al detal**: los granizados sin licor solo se venden al por mayor
  (mínimo 20 unidades sin licor). `finalize_checkout` rechaza de forma determinista un pedido
  sin licor por debajo del mínimo (`sin_licor_retail_block`, toggle `SIN_LICOR_RETAIL_AVAILABLE`).
- **Mínimo 24h para pedidos programados**: `set_delivery_schedule` ahora recibe fecha/hora en ISO
  (`YYYY-MM-DD` / `HH:MM` 24h) que el modelo resuelve, valida ≥24h de anticipación de forma
  determinista y guarda en ISO (las columnas tipadas de la BD ya se llenan).
- Al asesor se le muestra, cuando se usa un código de referido, el **código, el descuento al
  cliente y la comisión del embajador** en el resumen del caso (`advisor_case_summary`).
- Conversation trace for the CRM: new append-only `message_events` table (migration 010) logs every
  message that flows through the bot — the customer↔bot lane (`channel = 'client'`) and the internal
  bot↔advisor lane (`channel = 'advisor'`), tagged by `actor` (`client` / `bot` / `advisor`). Logged
  best-effort at the shared seams (`execute_actions`, `send_timer_actions`, inbound customer/advisor
  handlers, the agent welcome greeting, and the degradation fallback) so a logging failure never
  blocks delivery. `send_timer_actions` now takes the case phone so timer-driven messages attribute
  to the right conversation. New `record_message_event` query; pure classification helpers unit-tested.
- Advisor can send a bare `✅` to the bot to silently keep the WhatsApp 24h service window open
  (e.g. daily, before it lapses) without triggering any bot reply. `process_advisor_input` now
  short-circuits on `is_window_keepalive_ping()` in `src/engine.rs` before any routing/DB work.

### Changed
- **Motor por defecto ahora es Claude Sonnet 4.5** (`DEFAULT_MODEL`), antes Haiku 4.5 — mejor
  razonamiento aritmético/fechas para reducir errores de conteo y programación.
- El agente ahora envía **un solo mensaje de WhatsApp por turno** (se acumula el texto de todas
  las rondas del loop) en vez de una ráfaga de 2-3 mensajes.
- El **tool-result de resumen del pedido incluye el total de unidades** explícito para que el
  modelo no invente el conteo (dijo "45" cuando había 35).
- El **timer de espera del asesor pasó de 5 a 10 minutos**; al vencer, el cliente recibe un mensaje
  de "tu pedido quedó guardado, un asesor te escribe" en vez de "empieza de nuevo desde el menú".
- En modo agente, el **recordatorio por inactividad es texto** (antes reinyectaba botones/listas
  deterministas — origen de los botones de pago que aparecían en el CRM de prueba).
- Prompt reforzado: resolver zona de Armenia automáticamente sin preguntar al asesor, y regla de
  sin licor / mínimo 24h de programados.
- Documented `crm-web/` in the root README ("CRM web" section) and in `CLAUDE.md`'s code layout —
  it previously had no run instructions anywhere in the repo.
- Fixed a stale doc reference in `CLAUDE.md`: architecture diagrams are `general_info/*.md`, not
  `*.mermaid`.

### Removed
- Deleted the empty `.simulator_uploads/` directory and the unused `BOT_MODE`/`SIMULATOR_UPLOAD_DIR`
  entries from local `.env` — dead since the simulator removal in v1.8.0 (`config.rs` never reads
  either variable).
- Archived completed AI-agent rollout planning docs (`AI_AGENT_FAQ.md`, `MASTER_PROMPT.md`,
  `MASTER_PROMPT_PRODUCCION.md`, `todo.md`) into `docs/archive/` — all describe already-shipped,
  already-deployed work now superseded by `docs/project-knowledge/SESSION-*.md`.

## [1.8.0] - 2026-07-19

### Removed
- **The local simulator was removed entirely** (runtime, module, UI, launch scripts, assets, and
  docs). The bot is now production-only: there is no `BOT_MODE` split — the single runtime is the
  Meta Cloud API webhook path. Deleted `src/simulator/`, `src/routes/simulator.rs`,
  `src/transport.rs`, `assets/`, `scripts/run_simulator.*`. `OutboundTransport` (an enum whose only
  purpose was to switch Meta vs. simulator recording) is gone; `AppState.transport` is now the
  `WhatsAppClient` directly. `Config` lost `BotMode`/`SimulatorConfig`/`mode` and its
  `ProductionConfig` was flattened into `Config`. The simulator-only timer machinery in
  `bot/timers.rs` (`SimulatorTimerOverrides`, `TimerOverridesHandle`, `simulator_timer_rules`,
  `simulator_timer_snapshots`, `record_simulator_timer_notice`, and the `is_simulator` threading)
  was removed; production always uses the default timer durations, which is exactly what it did
  before. Validation is now `cargo test` + the deterministic-engine fallback + live testing.
  Migration `005_create_simulator_tables.sql` stays (append-only history); its tables are now
  orphaned but harmless.
- Removed the `FORCE_BOGOTA_NOW` env override and the simulator in-memory clock override in
  `bot/states/scheduling.rs`. `now_bogota()` is now always `Utc::now()` in `America/Bogota`.
- Removed the dead `calculate_order_with_delivery()` super-tool (and its `OrderSummary` struct)
  from `src/ai/tools.rs` — it was never wired into the agent tool dispatch.

### Added
- Customer totals and referral analytics now update when an order actually reaches `confirmed` (cash on delivery selected, or transfer receipt received), in both the AI-agent flow and the deterministic flow, via shared `checkout::order_confirmation_analytics_action()`. New `BotAction::UpdateCustomerAndAnalytics` executed in `engine.rs`. Integration tests in `tests/customer_analytics.rs` cover cumulative updates for both tables; unit tests in `checkout.rs` cover both confirmation paths.
- New `crm-web/` Next.js dashboard (separate app, shares the bot's PostgreSQL database via direct `pg` connection, no Supabase involved): customer search/sort, customer detail view with conversation transcript (parsed from `agent_case_messages`), order history, and referral-code usage per customer.
- Automatic capture of Meta WhatsApp username in webhook and persistent storage in `customers` table to enable customer identification by username as backup if phone changes.
- Permanent conversation memory: agent conversation history now persists indefinitely by customer instead of clearing after checkout, enabling full CRM view of all previous interactions.
- Persistent customer CRM data via new `customers` table (migration 008): tracks unique customer by `phone_number_meta` from Meta, with optional manual phone/name, username, last delivery address, and cumulative totals (spend and units). Supports cross-conversation history without limits.
- Referral code analytics via new `referral_code_analytics` table (migration 009): tracks usage count, total discounts generated, ambassador commissions, units purchased, and gross sales per code for business intelligence and commission reporting.
- Claude Haiku 4.5 AI agent mode (BOT_ENGINE=agent) for customer conversations: orchestrates menu selection, data collection (name, phone, address), order assembly, delivery-zone detection, and checkout with tool-calling. Agent handles customer/advisor message routing, confirms availability and payment method, and bridges to advisor. Deterministic pricing, delivery zones, referrals, and validation remain unchanged. Conversation locks prevent race conditions on concurrent messages from customer and advisor. Conversation memory persists agent history between turns in `agent_case_messages` table. New migration: `007_create_agent_case_messages.sql`. Configuration: `BOT_ENGINE` env var selects engine (works in the production runtime). New files: `src/ai/{client.rs,agent.rs,tools.rs,memory.rs}`, `src/bot/delivery_zone.rs`.
- Deterministic calculation tools for agent (FASE 2): `get_delivery_cost()` resolves delivery zones (Armenia norte/centro/sur, nearby towns, or manual unknown) and `apply_referral_discount()` applies referral codes with boost detection. Both delegate to existing pricing and delivery-zone logic; no rule changes. (A third `calculate_order_with_delivery()` super-tool was added here but never wired into dispatch, and was removed in 1.8.0.)

### Added
- Safe degradation when the agent LLM call fails (timeout, 5xx, exhausted credit): the customer
  receives a fixed message from the new `[agent].llm_failure_customer` entry in
  `config/messages.toml`, the advisor receives the case context (customer, phone, last message,
  state), and the conversation state is left untouched so the case resumes when the API recovers.
  The Anthropic HTTP client now has explicit timeouts (60s request / 10s connect) so a hung call
  can no longer hold a conversation lock indefinitely. Integration test in
  `tests/agent_degradation.rs`.
- LLM cost controls: (1) the full per-customer history in `agent_case_messages` is still persisted
  (CRM view unchanged) but only the last 40 messages are sent to the LLM per turn, cut at a safe
  tool-use boundary; (2) inbound text is truncated at 1,500 characters before reaching the LLM;
  (3) per-customer daily budget of 60 LLM calls — when exhausted the customer gets the fixed
  `[agent].daily_limit_customer` message and the advisor is notified once per day per case;
  (4) optional global daily kill-switch via `AGENT_DAILY_LLM_CALL_LIMIT` env var. Counters live in
  memory (reset on redeploy) keyed to the Bogotá calendar day. New module `src/ai/budget.rs`.
- Webhook deduplication by Meta `message_id` with an in-memory 10-minute TTL cache: a Meta retry
  of an already-processed webhook no longer produces duplicate replies or duplicate LLM calls.
- Anti prompt-injection section in the agent system prompt: customer messages can never change
  prices, discounts, zones, or business rules; only tool-returned figures may be quoted; customers
  claiming to be the advisor/owner are treated as customers. Prices were already deterministic —
  this protects tone and promises.
- `set_payment_method` (cash on delivery) now returns the authoritative final total in its tool
  result so the model's closing message quotes the exact figure instead of recalculating it
  (observed hallucinated total in simulator testing).
- Flow-integrity hardening from simulator testing of the agent engine: (1) `message_advisor` now
  also binds the advisor session to the case, so an advisor reply always routes back even if the
  model messaged the advisor without having called `finalize_checkout` (observed: stranded case
  with no order row and an unroutable advisor confirmation); (2) tool state-guards now evaluate
  the in-turn effective state, so a `finalize_checkout` → `confirm_advisor_availability` chain
  within a single turn is accepted; (3) the system prompt now carries a per-state flow hint
  (`ask_delivery_cost` / `select_payment_method` / `wait_receipt`) plus explicit rules that the
  order is not confirmed until `set_payment_method` succeeds and that availability questions to
  the advisor require `finalize_checkout` first; `confirm_advisor_availability` returns the
  authoritative total and error texts teach the model the correct recovery sequence.

### Changed
- Removed the `AgentEngineRequiresSimulator` config gate: `BOT_ENGINE=agent` now boots with `BOT_MODE=production` (Meta webhook runtime). `BOT_ENGINE` unset still defaults to `deterministic`, so removing the variable in Railway remains an instant rollback.
- Main menu now shows only "Hacer Pedido" and "Ver Menú" buttons; "Hablar con Asesor" button removed from menu (agent handles advisor requests based on text input in FASE 6).
- Granizado pricing: "Segundo con licor" renamed to "Par con licor" at $12.000 (2 units at half price).
- Order summary (`render_summary()`) now displays automatic delivery cost and referral discount breakdown inline instead of deferring to advisor; includes Subtotal, Domicilio, and Total with referral discount details when applicable.
- Agent system prompt now includes detailed instructions on: when to use `message_advisor()` (4 specific cases), majority-order rules with referral logic (20+ units same type), automatic delivery-zone handling (Armenia zones, nearby towns, unknown municipalities), and button vs. freetext interaction patterns.
- Lowered the per-phone daily LLM call budget from 60 to 30 (`PER_PHONE_DAILY_LIMIT` in `src/ai/budget.rs`) — 30 comfortably covers a full order conversation with headroom, while tightening the anti-abuse ceiling for the production canary.

### Fixed
- Customer inactivity timer reset conversations after 2 minutes without ever sending the reminder: the FASE 5 consolidation replaced the 35-minute reset window with the 2-minute reminder window in the expiration guard, making the reminder branch unreachable at natural expiration. The timer now sends the reminder exactly once and never resets the conversation (runtime, sweep, and boot reconciliation), matching the documented FASE 5 behavior. Dead 35-minute reset code (`CONVERSATION_RESET_TIMEOUT`, `reset_notice_actions`) removed.
- `confirm_advisor_availability()` recomputed `total_final` without subtracting an already-applied referral discount, overwriting the discounted total in the order and the customer-facing summary. The recomputation now includes the referral discount.
- Customer/referral analytics were recorded when the advisor confirmed availability (order still at `draft_payment`), so orders canceled at the payment or receipt step inflated `customers` totals and `referral_code_analytics`, referral codes applied after advisor confirmation were never counted, and the deterministic production flow never recorded analytics at all. The update now fires only on the two real confirmation transitions (cash on delivery, transfer receipt) in both engines.
- `create_or_update_customer()` and `create_or_update_referral_analytics()` used unqualified column references inside `ON CONFLICT DO UPDATE SET`, which Postgres treats as ambiguous between the target table and the implicit `excluded` row — every upsert attempt failed with a `42702` error. Both queries now qualify the target-table column explicitly. This had shipped in the same day's earlier commit and was never exercised against a live database until this session.
- **Canary fixes (2026-07-19), items 2/8/4/D/5 from `docs/canary-fixes-2026-07-19.md`** — real bugs found in live testing of `BOT_ENGINE=agent`:
  - The LLM narrated totals/figures without reading them from a tool-result (confirmed the deterministic pricing engine itself was already correct in every tested scenario). New deterministic guard (`extract_currency_amounts`/`known_tool_amounts`/`sanitize_hallucinated_amounts` in `src/ai/agent.rs`) blocks and replaces any outgoing message that mentions a `$` amount not backed by a real tool-result from the conversation, instead of relying on the prompt alone.
  - `checkout::render_summary()` treated "delivery cost not known yet" (`None`) the same as "known and free" (`Some(0)`), so a products-only subtotal could be shown labeled as `Total`. It now renders a distinct "Subtotal de productos (sin domicilio aún)" label until the delivery cost is actually known; `summary_template`'s `{total}` placeholder was replaced with `{total_line}` in `config/messages.toml` and the strict placeholder validator in `src/messages.rs`.
  - Scheduled orders were routed through `confirm_advisor_availability()` (the immediate-order "can staff deliver right now" check), violating the business rule that scheduled orders auto-accept and never wait on advisor confirmation. `finalize_checkout()` now auto-accepts scheduled orders once the delivery cost is known (new `auto_accept_scheduled_order()`/`compute_total_final()` helpers), `set_manual_delivery_cost()` does the same when the advisor supplies a cost for an unknown municipality after the order was already finalized, and `confirm_advisor_availability()` now explicitly rejects any call against a scheduled order. `try_handle_receipt_shortcut()` (receipt-photo forward to the advisor) no longer depends solely on the exact conversation state — it also triggers from context (`payment_method == transfer` and no receipt yet) so a state desync can't cause a receipt to go unforwarded.
  - Flavors that exist as distinct products in both liquor/non-liquor variants under a shared base name (Maracumango, Manzana verde, Bonbonbum — 3 variants, Blueberry) could be silently added with a guessed variant when the customer only said the base name. `add_order_item` now requires a `customer_wording` field (the customer's literal phrasing) and deterministically rejects the call if the base name is ambiguous and the wording doesn't disambiguate it (new `check_flavor_disambiguation()`/`AMBIGUOUS_GROUPS` in `src/ai/tools.rs`); flavors that only exist in one variant are unaffected.
- **Canary fixes (2026-07-20), items 1/3/6/7/9 + findings A/C from `docs/canary-fixes-2026-07-19.md`** — remainder of the agent-mode backlog:
  - **Duplicate confirmed order on adjustment (finding A, critical).** Confirming an order emitted `ResetConversation`, which made `engine.rs` skip persisting `context.to_state_data()` and wiped `current_order_id`; a later adjustment then created a *second* confirmed order in the DB and analytics double-counted. The two agent confirmation paths (cash, receipt) no longer reset — context persists with `current_order_id` intact. New `order_confirmed` flag + guard in `finalize_checkout()` rejects re-confirmation; new tools `modify_confirmed_order` (reopens the SAME order → UPDATE) and `start_new_order` (releases the binding for a separate order). Analytics switched to **delta**: new `confirmed_order_snapshot` (state_data) + `referral_times_used_inc` on `UpdateCustomerAndAnalytics` so a re-confirmation adds only the difference and never re-counts `times_used`. The deterministic engine never sets a snapshot, so its behavior is unchanged. Advisor is notified "✏️ Pedido MODIFICADO".
  - **Wholesale orders never asked for a referral/discount code (item 9).** Deterministic guard in `finalize_checkout()`: if the order has a wholesale bucket (`has_wholesale_bucket`) and the code question isn't resolved, confirmation is blocked. New `referral_prompt_resolved` flag set by a valid `apply_referral_code` or by the new `skip_referral_code` tool. Codes only apply to wholesale (retail was already rejected by pricing), so retail orders are never asked.
  - **Business hours not respected (item 1).** The current Bogotá day/time and open/closed status are now injected into the system prompt's "ESTADO ACTUAL DEL CASO" block every turn (`build_system_prompt`), so the LLM no longer answers from memory or depends on optionally calling `check_business_hours`. Also feeds correct date reasoning for scheduled orders (item 7).
  - **No mandatory final recap (item 7).** System prompt now requires recapping products+variant+quantity, exact absolute date/time, address, and delivery-inclusive total (and waiting for an explicit OK) before any confirmation, using the injected clock to resolve "mañana"/"hoy".
  - **All buttons removed in agent mode (item 3).** First contact in agent mode replies with a fixed welcome (`[agent].welcome` in `config/messages.toml`) without an LLM call (new `has_greeted` flag); everything after is LLM-driven text. The three timer sites that emitted buttons (receipt/contact/advisor timeout) now send plain text in agent mode (states are already agent-owned so text replies route to the LLM); the deterministic engine keeps its buttons.
  - **WhatsApp bold formatting (item 6).** New deterministic `normalize_whatsapp_markdown()` collapses `**x**`→`*x*` and `__x__`→`_x_` on every outgoing agent `SendText`, plus a prompt rule.
  - **Meta vs. custom customer data (finding C).** No migration needed — the `customers` table already had meta/manual columns. New immutable `meta_customer_name`/`meta_customer_phone` (seeded from Meta webhook); `set_customer_field` now stores custom name/phone without validation; the advisor packet shows both ("custom (Meta: real)") via `customer_identity_line`, and persistence maps meta vs. manual correctly (`manual_override`), so a fabricated value never replaces the real Meta data.

### Added
- Agent tools `modify_confirmed_order`, `start_new_order`, and `skip_referral_code`; fixed agent welcome message `[agent].welcome` in `config/messages.toml`.

## [1.7.2] - 2026-04-30

- Restore safe advisor routing by quoted Meta `context.id` for advisor replies while keeping active advisor-session fallback when no valid quote exists.
- Keep customer webhook intake tolerant of external Meta payloads with profile names, interactive messages, and missing or empty quoted context IDs, preserving the early `received whatsapp message` log.
- Add referral boost codes from `config/referrals.toml`; boost codes must also be valid referral codes and add 5 percentage points to ambassador commission without changing the customer discount.
- Update tracked referral codes to `trabix-prueba15`, `rider332`, and `bytebann`, with `trabix-prueba15` enabled as the boosted code.
- Use a 23-hour stale cutoff for scheduled-order `ask_delivery_cost` waits while keeping immediate delivery-cost waits on the existing 30-minute hard reset.
- Store new advisor reply-thread and referral boost metadata in `conversations.state_data` with legacy JSON defaults, avoiding a SQL migration.

## [1.7.0] - 2026-04-07

- Add a wholesale-only ambassador referral step before payment with `Tengo código` / `Seguir sin código`, lowercase code validation from `config/referrals.toml`, and retry/skip handling for invalid codes.
- Apply referral discounts only to wholesale-priced buckets, persist `referral_code` plus discount/commission totals in `orders` and `state_data`, and update final customer totals without discounting delivery cost.
- Show referral discount details in the customer payment-ready summary and include referral code plus ambassador accounting totals in advisor summaries once a valid code is used.
- Round the client referral discount up to the next `$100` so values like `$4.510` become `$4.600`, then recompute the paid subtotal and ambassador commission from that rounded discount.
- Restrict tracked referral codes to trimmed lowercase strings without spaces and with a maximum length of 15 characters.
- Add a simulator-only Bogotá clock override in the local UI so immediate-hours, out-of-hours, and scheduling flows can be validated without restarting the app or setting `FORCE_BOGOTA_NOW`.

## [1.6.1] - 2026-04-06

- Send the advisor a final confirmed-order packet with customer data, order details, and final totals when the customer completes payment by `Contra Entrega` or `Pago Ahora`.

## [1.6.0] - 2026-04-06

- Replace the old `show_summary` checkout split with a combined `review_checkout` step plus a final button-based payment selection after advisor handling.
- Move advisor delivery-cost capture before final payment, auto-accept scheduled orders after delivery cost, and remove the wholesale-specific checkout relay branch.
- Change immediate-order advisor waiting so a 5-minute silence auto-falls back to the same branch as `No puedo`, while `Hablar con Asesor` now exposes only `Atender` on the advisor side.
- Remove the manual advisor `No puedo` button from `wait_advisor_response` and keep the 5-minute timeout as the only fallback into hour negotiation.
- Stop re-sending relay `Finalizar` on every forwarded client message; the advisor now receives that button only on the initial relay handoff.
- Keep receipt upload at the end of the flow, forward uploaded receipts to the advisor, and update diagrams/runtime docs to match the new production flow.

## [1.5.2] - 2026-03-29

- Rename the tracked Mermaid flow documents to `general_info/complex_diagram.mermaid` and `general_info/simple_diagram.mermaid`.
- Update repository guidance and runtime reference docs so they point to the new diagram source-of-truth files.
- Fix Docker/Railway builds by copying `assets/` and `config/` into the builder image so compile-time `include_str!` simulator UI and message-config assets exist during `cargo build --release`.
- Copy `assets/` into the runtime image as well so simulator mode can still serve the tracked `assets/trabix-menu.png` menu file from disk.

## [1.5.1] - 2026-03-26

- Refactor the simulator boundary so HTTP handlers move into `src/simulator/web.rs`, leaving `src/routes/simulator.rs` as a thin mount wrapper while keeping the shared production bot brain unchanged.
- Extract the simulator frontend into editable files under `assets/simulator/` (`index.html`, `simulator.css`, `simulator.js`) and serve them through dedicated simulator asset routes.

## [1.5.0] - 2026-03-26

- Upgrade the local simulator UI so the advisor pane is session-centric, the old advisor inbox panel is removed, and timer-driven transcript updates appear without manual page refresh.
- Add a read-only database inspector inside `/simulator` for raw `conversations`, `orders`, and `order_items` rows, plus backend list queries for those tables.

## [1.4.6] - 2026-03-26

- Finalize the public README onboarding and remove an accidentally tracked simulator upload artifact from the repository so the public tree matches the intended local-only simulator workflow.

## [1.4.5] - 2026-03-26

- Add a top-level `README.md` that explains the project, the proprietary repository terms, and how to run the real production bot brain locally through the simulator on macOS, Linux, and Windows.
- Document simulator boundaries clearly so users understand that shared bot logic is reused locally while Meta-specific transport behavior still requires real WhatsApp validation.

## [1.4.4] - 2026-03-26

- Replace the simulator placeholder menu asset with the real tracked menu image at `assets/trabix-menu.png`.
- Keep simulator menu serving fixed to the tracked repository asset so every clone sees the same menu by default.

## [1.4.3] - 2026-03-26

- Remove `SIMULATOR_MENU_IMAGE_PATH` from simulator configuration and always serve the tracked fallback asset for `Ver Menú`.
- Keep the cross-platform simulator launchers but simplify them to the fixed tracked menu asset workflow so teams can replace that file and push it with the repository.

## [1.4.2] - 2026-03-26

- Add cross-platform simulator launcher scripts for macOS/Linux (`scripts/run_simulator.sh`) and Windows (`scripts/run_simulator.ps1`, `scripts/run_simulator.bat`) that preconfigure simulator env vars and can auto-start a local Postgres container when Docker is available.
- Add a tracked placeholder menu asset for local simulator bootstrapping and support serving `.svg` menu assets in the simulator.

## [1.4.1] - 2026-03-26

- Add a top-level proprietary `LICENSE` with `All Rights Reserved` terms and a narrow evaluation-only permission to view the repository and run the local simulator for personal testing.

## [1.4.0] - 2026-03-26

- Add simulator-side timer observability with per-message Bogota timestamps, active timer panels, deadline/countdown display, and simulator-only timer speed overrides from the local UI.
- Make timer restoration and sweep logic simulator-override aware so local timeout validation follows the same shared engine path while remaining isolated from production transport.
- Record simulator timer system notices so receipt, advisor, relay, and inactivity expirations show whether they came from runtime, sweep, or boot reconciliation.

## [1.3.0] - 2026-03-26

- Add a local `BOT_MODE=simulator` runtime that serves `/simulator` and exercises the same bot state machine, PostgreSQL persistence, advisor flow, pricing, and timers without calling Meta.
- Refactor inbound processing and outbound action execution into a shared engine so webhook messages, simulator messages, and timer expirations all use the same runtime path.
- Add simulator transcript and media persistence with new PostgreSQL tables for local sessions, chat history, and uploaded receipt/image files.
- Add an Axum-served local chat UI with multi-session customer testing, advisor interaction, button/list replay, local image upload, and persisted state inspection.

## [1.2.0] - 2026-03-25

- Set every SQLx PostgreSQL session to `America/Bogota` so app-driven `NOW()` and timestamp display align with `UTC-5` operations.
- Remove the standalone `Horarios` menu flow, move the immediate-delivery hours into the welcome message, and switch the main menu to 3 WhatsApp buttons.
- Add a generic customer inactivity timer: resend the current prompt once after 2 minutes, then reset to `main_menu` after 35 minutes without customer activity.
- Add a 30-minute hard reset for stuck advisor-detail waits (`ask_delivery_cost`, `negotiate_hour`, `wait_advisor_hour_decision`, `wait_advisor_confirm_hour`) and move timed-out orders to `manual_followup`.

## [1.1.2] - 2026-03-22

- Add a periodic database-backed timer sweep so receipt, advisor, and relay expirations still fire if an in-memory task is missed.
- Keep the existing boot-time timer restoration and make timeout handling more resilient after deploys or runtime interruptions.

## [1.1.1] - 2026-03-13

- Fix Railway startup by restoring the original checksum for migration `002_create_orders.sql`.
- Keep the new order schedule text columns exclusively in migration `004_add_order_schedule_text.sql`, which is safe for existing databases and fresh installs.

## [1.1.0] - 2026-03-13

- Process every inbound WhatsApp message in batched webhook payloads instead of dropping all but the first.
- Resume timed-out wholesale scheduling through the correct advisor state.
- Preserve accepted free-form scheduled date and time values in persisted orders.
- Move the remaining receipt-timeout prompt body into message configuration.

## [1.0.0] - 2026-03-13

- Baseline release of the Rust WhatsApp ordering bot before the post-release workflow bugfixes.
- Includes the implemented customer flow, checkout foundation, and advisor/relay logic currently present in the repository state prior to the `v1.1.0` fixes.
