# ROADMAP — `trabix-bot`

> Léeme al iniciar sesión, junto con `CLAUDE.md`. Este archivo dice **qué sigue y en qué orden**;
> `CLAUDE.md` dice **cómo está construido**. Actualizar este archivo cuando algo se complete.
>
> Última revisión: 2026-09-08 (ver punto 5). Los puntos 1–3 siguen fechados 2026-08-12 y tienen
> datos desactualizados sobre embajadores — ver la nota al final del punto 4.

## Contexto de negocio mínimo (para no tener que salir del repo)

Trabix vende granizados en sachet sellado, Armenia (Quindío). Costo ≈ **$2.000 COP/unidad**.
**Prioridad única hoy: vender.** Solo hay dos canales activos: **retail por este bot** y **eventos**.
El programa de embajadores **no está corriendo** (los códigos de `config/referrals.toml` son
legacy/test) y los puntos de venta en tiendas están descartados. No propongas trabajo que dependa
de embajadores activos.

Está en marcha una inversión de **$2M COP, 80% en pauta Meta click-to-WhatsApp, solo Armenia**.
Todo lo de abajo existe para que esa pauta sea rentable. El bot es el destino de cada clic pagado.

Precios vigentes: retail **con licor** $8.000/u (2ª a mitad de precio). **Sin licor es solo
mayorista**, mínimo 20 u — el bot ya lo bloquea de forma determinista (`finalize_checkout`, toggle
`SIN_LICOR_RETAIL_AVAILABLE`). Mayorista con licor: 20–49 → $4.900, 50–99 → $4.700, 100+ → $4.500.

---

## Ya shippeado (no repetir)

- **`POST /internal/advisor/send`** (v1.11.0): endpoint interno autenticado con `X-Internal-Token`
  (`INTERNAL_API_TOKEN`, opcional → sin ella el endpoint queda deshabilitado en 503) para que
  `crm-app` mande WhatsApp al cliente a través del bot, sin volverse un segundo escritor sobre la
  conversación. Contrato completo en `docs/internal_advisor_send.md`. **Ya operativo:**
  `INTERNAL_API_TOKEN` cargada en Railway en el bot y en `crm-app`, y el camino corrió de verdad en
  producción el 2026-08-01 (`message_events` id 32: `actor='advisor'`, `payload.source='crm-app'`,
  `wa_message_id` real de Meta). `crm-app` lo llama por la **red privada**
  (`http://trabix-bot.railway.internal:8080`), así que ese tráfico ya no sale a internet.
  ✅ **Cerrado (Fase 8):** `/internal/*` ya no se sirve desde el listener público. Hay un segundo
  listener en `INTERNAL_PORT` (default `8081`) al que Railway no le asigna dominio, así que solo se
  alcanza por la red privada (`trabix-bot.railway.internal:8081`). Confirmado en los logs de
  arranque de cada deploy: `internal server listening on 0.0.0.0:8081 (private network only)`.
- **Prompt caching** (v1.9.0): `cache_control` en el system prompt estático + tools de
  `src/ai/agent.rs`. Ver `docs/PENDIENTE_prompt_caching.md`.
- **Domicilio gratis Armenia (6–19u) + detal sin mínimo en pueblos aledaños** (v1.9.0):
  `src/bot/delivery_zone.rs`. Ver `docs/PENDIENTE_domicilio_gratis.md`.
- **BOT_ENGINE toggle eliminado**: el bot corre agente siempre; `ANTHROPIC_API_KEY` es obligatoria.
  `reminder_actions()` (dead code real, no solo teórico) y sus helpers de botones huérfanos en
  `timers.rs` también se borraron. ⚠️ La variable `BOT_ENGINE=agent` **sigue cargada en Railway** y
  ya no la lee nadie — borrarla en el próximo deploy del bot (no vale un redeploy solo para eso).
- **`send_quick_replies`/`send_options_list`** conectados como tools del agente (hasta 3 botones /
  10 filas — límites de WhatsApp), sobre los `BotAction::SendButtons`/`SendList` que ya existían.
- **Migración `012`**: `DROP TABLE` de las tablas huérfanas del simulador que creó `005`.
- **CTWA click ID — captura sin credenciales**: `ctwa_clid` se lee del `referral` del webhook y se
  guarda en `customers` (migración `011`, se captura una sola vez, no se sobreescribe). Cliente CAPI
  (`src/capi.rs`) construido y wireado en la confirmación de compra (`UpdateCustomerAndAnalytics` en
  `src/engine.rs`) — hoy es un no-op silencioso porque falta configurar `WABA_ID` /
  `META_CAPI_DATASET_ID` / `META_CAPI_ACCESS_TOKEN`.
- **Fase 5: direcciones guardadas del cliente** (v1.16.0): tabla `customer_addresses` (migración
  `015`, máx. 4 por cliente, zona ya resuelta reconstruible desde `bot::delivery_zone`, costo
  guardado solo informativo). Tools nuevas `list_saved_addresses`/`select_saved_address`, hook en
  `confirm_order_bookkeeping` al confirmar pedido. `customers.delivery_address_last` no cambió.
  Detalle en `general_info/current_runtime_reference.md` → "Direcciones Guardadas (Recompra)".
  Del lado `crm-app` falta ver su propio ROADMAP (Contactos repunteado a `customers` +
  direcciones guardadas de solo lectura).
- **Fase 2: toma de control humana con auto-devolución** (v1.15.0): `conversations.human_takeover_until`
  (migración `014`), marcado por `POST /internal/advisor/send` (ventana deslizante, default 6h vía
  `ADVISOR_TAKEOVER_HOURS`), consultado en `engine::process_customer_input` y en los 4
  `expire_*_with_source` de `bot::timers` + la reconciliación de boot. `POST /internal/advisor/release`
  la libera antes de tiempo. `POST /internal/advisor/reply` NO la dispara a propósito — ver
  `docs/internal_advisor_send.md`. Del lado `crm-app` falta el composer (ver su propio ROADMAP).
- **Envío nacional** (v1.19.0): tercera forma de entrega (junto a Armenia y los 13 municipios).
  Tool `set_delivery_national` (`src/ai/agent.rs`) — mínimo 20 unidades sin excepción
  (`bot::delivery_zone::MIN_UNITS_NATIONAL`), no calcula tarifa (la cotiza el asesor vía
  `message_advisor` + `set_manual_delivery_cost`, reusando el camino ya existente de domicilio
  manual sin duplicar lógica), y obliga a decirle al cliente que el producto llega
  **descongelado** (promesa distinta a Armenia/municipios). Detalle en
  `general_info/current_runtime_reference.md` → "Envío Nacional". **Confirmado (Samuel,
  2026-08-02): la transportadora sí despacha con licor**, así que el canal arranca con el mismo
  catálogo que el resto del retail, sin restricción de alcohol. `website/retail/index.html` y el
  `CLAUDE.md` de la raíz ya están sincronizados.
- **Fase 6 (lado bot): códigos de referido en base de datos** (v1.17.0): reemplaza
  `config/referrals.toml` por la tabla `referral_codes` (migración `016`, sembrada con los 5
  códigos legacy), cacheada en memoria (`src/referrals.rs`, refresco en background cada 30s +
  refresco inmediato tras cada escritura) — cambiar un código o activar un boost ya no exige
  desplegar el bot. El boost pasa de ser un flag fijo (`boost_codes` en el TOML) a una ventana real
  de 7 días (`boost_until`) que expira sola. 3 endpoints internos nuevos
  (`POST /internal/referral-codes`, `PATCH /internal/referral-codes/:code`,
  `POST /internal/referral-codes/:code/boost`), mismo `X-Internal-Token` que
  `/internal/advisor/send` — el bot sigue siendo el único escritor de la tabla. Del lado `crm-app`
  falta la sección "Embajadores" que llama a estos endpoints (ver su propio ROADMAP).
- **Retiro del canal WhatsApp directo del asesor, ejecutado** (v1.22.0, 2026-08-02):
  `ADVISOR_WHATSAPP_ENABLED` eliminado del código, ya no queda ninguna condición que trate
  `ADVISOR_PHONE` como especial en el enrutamiento inbound. Nuevo `BotAction::NotifyAdvisor` escribe
  directo a `message_events` (carril `advisor`/`actor='bot'`) sin tocar Meta. `crm-app` es ahora la
  única superficie de trabajo del asesor. **Deliberadamente sin tocar**: `src/bot/states/advisor.rs`
  (~2.100 líneas) y `relay.rs` (~270), el FSM determinístico legado — sigue siendo código vivo para
  ~15 estados de hand-off no agent-owned. Esa limpieza sigue siendo la sección 2 de abajo.
- **Confirmación de pago manual, cambio de regla de negocio ejecutado** (v1.23.1, 2026-08-12): subir
  el comprobante YA NO confirma el pedido automáticamente. El pedido queda en `waiting_receipt` hasta
  que el asesor verifica la plata en el banco y lo confirma desde `crm-app` (nueva tool
  `confirm_payment_received`, solo en turno de asesor). El despacho (`order_dispatch`) sigue siendo
  un paso aparte, sin cambios.
- **Proxy de medios de WhatsApp para `crm-app`** (v1.23.1): `GET /internal/media/:media_id` — el bot
  resuelve y descarga el adjunto desde la Graph API y lo sirve con el `Content-Type` correcto, porque
  `crm-app` no tiene credenciales de Meta propias. `crm-app` ya renderiza `<img>` real en vez de la
  etiqueta "📎 Imagen".
- **E2E round 2 (2026-08-12): 3 bugs reales encontrados y corregidos en vivo** — ya pusheados y
  desplegados (ver `~/.claude/plans/e2e-advisor-push-test-round2.md` para el
  detalle completo de la sesión):
  - **v1.23.2** — el saludo fijo de primer contacto (`engine.rs`, ahorra una llamada al LLM) dejaba
    el primer mensaje del cliente ausente de `agent_case_messages` (memoria propia del LLM), aunque
    sí quedaba en `message_events`. Si el cliente ya pedía algo en ese primer mensaje, el modelo
    genuinamente no lo veía en el siguiente turno. Nuevo `ai::agent::record_greeting_turn` persiste
    ambos lados de ese turno antes de retornar.
  - **v1.23.3** — regresión real de v1.23.1: con la confirmación manual, el pedido se queda a
    propósito en `wait_receipt` esperando verificación — pero el sweep periódico que recupera timers
    tras un restart/tick (`timer_recovery` en `src/bot/timers.rs`) no sabía que el timer en memoria ya
    se había cancelado al llegar la imagen, y volvía a armar un timer fantasma de 10 minutos. Al
    cliente le llegó un falso "no recibimos el comprobante" 10 minutos después de haberlo mandado.
    Corregido: el sweep y el boot-restore ahora también exigen `receipt_media_id.is_none()`.
  - **v1.23.4** — hallazgo colateral, no relacionado al motor: una edición manual de
    `config/messages.toml` esa misma mañana (commit `c783cb1`) movió `transfer_payment_text` fuera de
    la tabla `[checkout]` por accidente; TOML lo re-escopeó bajo `[timers_customer]`. Como ese campo
    es requerido (no `Option`), habría tumbado el bot completo en el próximo deploy — nunca llegó a
    producción porque ese commit nunca se hizo push. Detectado por 74 tests fallando en
    `cargo test`. Solo se corrigió la ubicación del campo.
  - **Resultado del test E2E**: pasos 1–4 (escalamiento a `needs_human`, cotización desde `crm-app`,
    confirmación manual de pago, despacho) verificados end-to-end con datos frescos tras el fix de
    cada bug — todos correctos. Paso 5 (toma de control humana + "Devolver al bot") confirmado por
    Samuel vía uso general previo, no vía una traza fresca de esta ronda específica (no hay eventos
    de `human_takeover_until` en este conversation_id posteriores al despacho).
  - **Pendiente de decisión de Samuel**: hacer push (dispara auto-deploy en Railway) de los 3 commits
    de fix (`297625f`, `9817504`, `51e19f4`) — todavía viven solo en `master` local.
  - **Feedback de producto, no bloqueante**: Samuel pidió que los mensajes internos que sí requieren
    su acción (p. ej. "¿cuál es el costo de envío?", "confirma el pago") se distingan en el
    `/pendientes` de `crm-app` de los que son solo informativos. Hoy no existe ninguna señal
    determinística para esa distinción — `needs_human` es un flag derivado de SQL (última fila del
    asesor en el carril `advisor` es más reciente que la última respuesta del asesor), y
    `message_advisor` es texto libre del LLM sin tipo/tag. Requeriría que el bot etiquetara la tool
    con algo como `requires_action: bool` al escribirla. Ver ROADMAP de `crm-app` para el seguimiento.

---

## Orden de ejecución

### 1. CTWA + Conversions API — credenciales ya cargadas, falta un pedido real vía anuncio

**Todo el código está listo** (captura de `ctwa_clid`, migración, cliente CAPI fail-silent, wireado
al momento real de compra) y **`WABA_ID`/`META_CAPI_DATASET_ID`/`META_CAPI_ACCESS_TOKEN` ya están
cargadas en Railway** (verificado 2026-08-12). El pipeline se probó de extremo a extremo en la
ronda de test E2E del 2026-08-12: al confirmarse el pedido de prueba (id 35), el bot sí llamó a
`capi::report_purchase` y sí llegó a Meta — Meta respondió `400 Missing Ctwa Clid` porque ese pedido
no vino de un clic real en un anuncio (era una conversación de prueba manual), lo cual es la
respuesta correcta y esperada, no un fallo: confirma que el dataset ID y el access token son
válidos y que Meta procesa el payload.

Lo único que falta:
1. Verificar en la consola de Meta (Events Manager → dataset) que un evento `Purchase` real
   (`ctwa_clid` presente) llega y se registra, con un pedido que sí haya entrado por un clic de
   anuncio — bloqueado en que la campaña de Meta Ads esté corriendo, no en código.
2. **Cambiar el evento de optimización** en Meta de "Conversaciones" a "Compras" cuando haya volumen
   (~50 compras/semana).

Detalle completo: `../docs/PENDIENTE_capi_meta.md`.

---

### 2. Limpieza del motor determinístico — Fase 1 ejecutada, queda la función-por-función

**Decisión de Samuel (2026-07-31), ejecutada (v1.22.0, 2026-08-02):** el bot ya no le manda
WhatsApp al asesor; `crm-app` es la única superficie de trabajo del asesor. `ADVISOR_WHATSAPP_ENABLED`
salió del código, `crm-app` está desplegada (Railway, deploy manual `railway up`, no git-connected)
y probada punta a punta en producción varias veces, incluyendo la ronda de test E2E del 2026-08-12
(ver "Ya shippeado" arriba). Esto ya no es un bloqueo — lo que sigue es la limpieza de código muerto
de abajo, que puede avanzar en cualquier momento sin depender de nada externo.

**Ronda función-por-función ejecutada (2026-08-12, v1.23.5–v1.23.11)**: `transition()` sigue vivo
hoy para los ~15 estados de hand-off no agent-owned (`WaitAdvisorResponse`, `RelayMode`,
`SelectReferralOption`, etc. — confirmado con una conversación real de producción, `573219864356`,
todavía sentada en `wait_advisor_response` desde 2026-03-23; ese subgrafo **no se tocó**, sigue
siendo alcanzable). Lo que sí se confirmó y se borró: los 25 handlers que `transition()` despachaba
para estados **agent-owned** — provablemente muertos, porque `should_use_agent`
(`engine::is_agent_owned_state`) intercepta esos estados antes de que `transition()` se llame nunca
con ellos. Verificado función por función con grep (no por archivo), sus arms en `transition()` ahora
son `unreachable!()` con el motivo citado inline. Resultado: **~1.966 líneas netas borradas** (25
handlers + ~20 helpers `*_actions`/render/validación que quedaron huérfanos + 32 tests que solo
probaban código muerto), `cargo test` en 200/0/5 (antes 232/0/5). Commits `1e85124`…`5dbea55`, ya pusheados a `master`.

**~~Lo que queda pendiente de esta limpieza~~ — HECHO (2026-08-25, v1.23.17).** Las 4 funciones de
`advisor.rs` (`start_waiting_for_contact_advisor`, `final_order_packet_actions`,
`render_final_order_status`, `render_contact_request`) y las 2 de `checkout.rs`
(`confirm_address_actions`, `change_address_prompt_actions`) se verificaron caller-by-caller con
grep y se borraron: forman un grupo muerto cerrado. De paso salió el import `customer_data` que
quedó sin uso y `ToolOutcome::ResultWithActions` en `src/ai/agent.rs` (variante nunca construida,
preexistente y sin relación con esta limpieza). **`cargo check` corre ahora sin ningún warning**, y
`cargo test` sigue en 200/0/5.

`transition()` en sí sigue sin poder borrarse completo (sección 5 de
`docs/CLEANUP_deterministic_engine.md` explica por qué: separar `ConversationState` en dos enums es
un cambio de diseño más grande, fuera de alcance de una limpieza mecánica).

---

### 3. Cosas menores sueltas

- Docs obsoletos (`docs/archive/MASTER_PROMPT.md`, etc.) — ya archivados, candidatos a borrar
  cuando alguien pase por ahí. Cero urgencia.

---

### 4. Timer "¿sigues por ahí?" ya no molesta tras gestionar el pedido — HECHO, falta QA end-to-end (2026-08-31, v1.23.18)

`sync_customer_inactivity_timer` (`src/bot/inactivity.rs`) y el path de recuperación al boot/sweep
(`order_already_gestioned`, `src/bot/timers.rs`) dejan de armar el recordatorio de 2 min una vez
`checkout_precondition_error` da `None` (ítems, datos del cliente y entrega ya conocidos) — antes
solo se suprimía en la transición puntual justo después de confirmar. 8 tests unitarios nuevos en
verde; **el path de recuperación al boot/sweep no se probó contra un pedido real** (no hay forma de
hacerlo desde `crm-app`, que nunca escribe en estas tablas). Plan de QA compartido con `crm-app`
(sembrar un pedido de prueba, verificar Pendientes/ventas ahí, y de paso este timer) en
`../crm-app/docs/qa_pendientes_ventas_2026-08-31.md`, sección 8.

Nota aparte: **este archivo está desactualizado en varios sitios** (dice "embajadores no está
corriendo" y `config/referrals.toml` como fuente de códigos — ambos cambiaron con la Fase 6 de
`crm-app`, código de referido ahora vive en la tabla compartida `referral_codes`). No se corrigió
en esta pasada por no ser el foco de la sesión; hace falta una revisión completa de este ROADMAP
contra el estado real del programa de embajadores.

---

### 5. Recompra que sobrescribía el pedido anterior — CERRADO (2026-09-08, v1.24.0 y v1.25.0)

Salió revisando por qué el chat de un cliente real (Kall Díaz) se había quedado sin bot. El
autopsy completo está en `docs/incidente_pedido_sobrescrito_2026-09-08.md`; el resumen es que
`current_order_id` sobrevivía indefinidamente al checkout, así que una recompra días después
dependía de que el modelo eligiera `start_new_order` y no `modify_confirmed_order`. Eligió mal: la
orden anterior quedó reescrita, la venta nueva ($104.000, 20 u) nunca apareció en Pendientes y no
llegó al sistema financiero.

Shippeado y **desplegado en producción** (deploy `d3134b3c`, 2026-09-08):

- `release_delivered_order_binding` corre al inicio de cada turno, antes de que el modelo vea nada,
  y suelta el binding cuando el pedido confirmado ya se entregó (inmediato +6h, o programado con la
  fecha pasada). Campo nuevo `order_confirmed_at`.
- Un pedido MODIFICADO notifica con `requires_action: true` → vuelve a Pendientes de `crm-app`.
- `free_delivery_status_line` en cada resumen del pedido: el estado del domicilio gratis lo resuelve
  el código, no el modelo.
- `modify_confirmed_order` enumera los items que ya están en el carrito.
- Pedido programado: 3h de anticipación en vez de 24 (`SCHEDULED_MIN_LEAD_HOURS`).
- `PER_PHONE_DAILY_LIMIT` 30 → 50.
- Bloque `ESTILO DE RESPUESTA` en el prompt: respuestas cortas, sin narrar el proceso interno.
- `claude-sonnet-4-5` → `claude-sonnet-5` con thinking adaptativo y `effort: low`; `max_tokens`
  1024 → 4096; el consumo por llamada pasa de `debug` a `info`.

Datos remediados en producción el mismo día: venta de la segunda compra registrada (factura 29, 20 u,
FIFO consumido), `customers` de Kall corregido a 40 u / $208.000, y su conversación reiniciada.

**Lo que sigue abierto de este hilo (no lo cerró este trabajo):**

- **Cancelar un pedido desde Pendientes no le avisa nada al cliente.** `crm-app`
  (`apps/crm/src/server/inbox/order-resolution.ts`) solo escribe la fila de `order_dispatch`; el bot
  ni se entera (`orders.status` sigue en `confirmed`) y la conversación sigue creyendo que el pedido
  va en camino. Hoy la única salida es tomar control manual y avisarle a mano. Relacionado: desde la
  Fase 1 el bot **auto-acepta todo** pedido inmediato en horario, así que no existe ningún camino
  por el que el asesor pueda decir "no puedo gestionarlo".
- **Medir el costo real por conversación con Sonnet 5.** Ahora que el consumo se loguea a `info`,
  hace falta mirar `input_tokens`/`output_tokens`/`cache_read_input_tokens` de unas cuantas
  conversaciones reales y actualizar las cifras de negocio, que están fechadas en Sonnet 4.5 sin
  caching y por tanto infladas. Si `cache_read_input_tokens` sale consistentemente en cero, el
  cambio de modelo dejó el caching sin efecto y hay que revisar el breakpoint.
- **El pedido 36 quedó representando la compra del 09-05, no la del 08-30.** Los sabores del primer
  pedido solo existen en `message_events`. No se restauró.

---

## Definición de "listo" para cualquier cambio aquí

`cargo check` + `cargo test` en verde · `CHANGELOG.md` actualizado · versión bumpeada en
`Cargo.toml` · `general_info/current_runtime_reference.md` actualizado si cambió comportamiento de
runtime · commit directo a `master` (no hay flujo de PR en este repo).
