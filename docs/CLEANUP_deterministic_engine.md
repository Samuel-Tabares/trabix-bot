# Limpieza del motor determinístico — estado real (actualizado 2026-07-31)

Este documento reemplaza la versión de 2026-07-25/26. Esa versión asumía que borrar el FSM
determinístico era tan simple como "grep qué importa `src/ai/agent.rs` de cada archivo y borrar el
resto". Al ejecutar esa verificación en esta sesión se encontró que el supuesto es **falso**: hay
una segunda vía de alcance, `src/bot/inactivity.rs` y `transition()` mismo, que mantiene vivo mucho
más código FSM del que el análisis original contemplaba. Ver sección 1.

## 0. Lo que SÍ se ejecutó en esta sesión (seguro, verificado)

1. **Se eliminó el toggle `BOT_ENGINE`** (`src/config.rs`, `src/engine.rs`, `src/bot/timers.rs`,
   `.env.example`, tests). El bot ahora corre agente siempre; `ANTHROPIC_API_KEY` es obligatoria.
   Esto era seguro porque `transition()` ya se ejecutaba para los mismos estados sin importar el
   valor del toggle (ver punto 1 abajo) — quitar el toggle no cambia ningún comportamiento
   observable, solo colapsa una rama que ya era inalcanzable en producción (`BOT_ENGINE` siempre
   fue `agent` desde el rollout).
2. **Se borró `reminder_actions()`** (`src/bot/inactivity.rs`) y los helpers de botones huérfanos
   en `timers.rs` (`receipt_timeout_buttons`, `advisor_timeout_buttons`, `contact_timeout_buttons`,
   `reply_button`). Esto SÍ era código muerto real: `reminder_actions` solo se llamaba desde la
   rama `else` de `if state.config.bot_engine.is_agent() { texto } else { reminder_actions(...) }`
   en `expire_conversation_abandon_with_source` — con el toggle fuera, esa rama ya no existe.
3. **Migración `012_drop_simulator_tables.sql`**: `DROP TABLE` de lo que creó la migración `005`
   (simulador, removido en v1.8.0). `005` se mantiene intacta (regla append-only).
4. **`send_buttons`/`send_list` conectados como tools del agente** (`send_quick_replies` con hasta
   3 botones, `send_options_list` con hasta 10 filas — límites duros de WhatsApp) en
   `src/ai/agent.rs`. Antes eran dead-code (`#![allow(dead_code)]` en `src/whatsapp/buttons.rs`)
   usados solo por el FSM determinístico.

## 1. Por qué NO se borró el resto (menu.rs, customer_data.rs, handlers FSM) — hallazgo clave

`transition()` (`src/bot/state_machine.rs`) **sigue siendo código vivo en producción, sin
importar el toggle**, porque `should_use_agent()` (`src/engine.rs`) solo desvía al agente cuando el
estado actual está en `is_agent_owned_state()`. Varios estados quedan **fuera** de esa lista y por
lo tanto siguen yendo a `transition()` cuando el CLIENTE escribe estando en ellos:

`SelectReferralOption`, `WaitReferralCode`, `WaitAdvisorResponse`, `NegotiateHour`,
`OfferHourToClient`, `WaitClientHour`, `WaitAdvisorHourDecision`, `WaitAdvisorConfirmHour`,
`WaitAdvisorMayor`, `RelayMode`, `ContactAdvisorName`, `ContactAdvisorPhone`, `WaitAdvisorContact`,
`LeaveMessage`, `OrderComplete`.

Es decir: mientras un caso espera respuesta del asesor, está negociando hora, o está en modo relay,
si el CLIENTE (no el asesor) escribe algo, ese mensaje se procesa vía `transition()` →
`checkout::handle_wait_advisor_response`, `advisor::handle_client_waiting_state`,
`relay::handle_relay_mode`, `checkout::handle_order_complete`, etc. Esto es verdad **hoy**, con el
toggle ya eliminado — no es un artefacto del `BOT_ENGINE` viejo.

Segunda vía de alcance, independiente de la primera: antes de esta sesión, `src/bot/inactivity.rs`
tenía `reminder_actions()`, que despachaba por *cada* `ConversationState` determinístico
(`MainMenu`, `ReviewCheckout`, `SelectType`, `CollectName`, etc.) hacia `menu::`, `checkout::`,
`order::`, `data_collect::`, `scheduling::`, `customer_data::`, `advisor::` — y esa función SÍ se
llamaba en producción (rama `else` del inactivity timer) hasta que se borró en el punto 0.2. Con
`reminder_actions` fuera, esta segunda vía de alcance desaparece — pero mientras existió, cualquier
intento de borrar esos módulos habría roto el recordatorio de inactividad del cliente.

**Conclusión:** con el toggle y `reminder_actions` fuera, la superficie de código FSM que sigue
viva se redujo a la primera vía (`transition()` para los ~15 estados no agent-owned de arriba) más
lo que las funciones `*_actions` de esos módulos comparten con `advisor.rs` (p. ej.
`advisor.rs` llama `checkout::payment_entry_state_and_actions` directamente, sin pasar por
`transition()`). Pero **archivo por archivo** (`menu.rs`, `customer_data.rs`, `order.rs`,
`data_collect.rs`, `scheduling.rs`, `checkout.rs`) siguen mezclando funciones vivas y muertas, y
distinguirlas requiere el mismo ejercicio de trazar cada función que se hizo en esta sesión para
`inactivity.rs` — no se alcanzó a completar para los seis archivos en esta sesión.

## 2. Qué queda pendiente y cómo abordarlo correctamente

**No repetir el error del documento anterior**: no asumir que "sin imports desde `src/ai/`" =
"código muerto". El criterio correcto es "sin ningún llamador vivo", y hay que verificarlo con
`grep -rn "nombre_de_la_función" src/` para cada función candidata, no por archivo.

Plan sugerido para la próxima sesión de limpieza:

1. Para cada archivo mixto (`checkout.rs`, `order.rs`, `data_collect.rs`, `scheduling.rs`,
   `customer_data.rs`, `menu.rs`), listar sus funciones `pub fn` y `grep -rn` cada una por fuera del
   propio archivo.
2. Clasificar cada función en tres grupos:
   - **Viva vía `transition()`** para uno de los ~15 estados no agent-owned (sección 1) → no tocar.
   - **Viva vía otro módulo** (`advisor.rs`, `src/ai/agent.rs`, `src/ai/tools.rs`) → no tocar.
   - **Sin llamadores fuera de su propio archivo, y solo alcanzable antes desde `reminder_actions`
     o desde un `handle_*` que ya está en el grupo muerto** → candidata a borrar.
3. Los `handle_*` que despachan estados agent-owned (`handle_review_checkout`,
   `handle_select_payment_method`, `handle_wait_receipt`, `handle_main_menu`, `handle_view_menu`,
   `handle_view_schedule`, `handle_select_type/flavor/quantity/add_more/confirm_restart_order`,
   `handle_collect_name/phone/address`, `handle_when_delivery/check_schedule/out_of_hours/
   select_date/select_time/confirm_schedule`, `handle_confirm_customer_data/
   select_customer_data_field/edit_customer_*`, `confirm_checkout`) son los primeros candidatos
   fuertes: solo los llama `transition()`, y `transition()` nunca los alcanza en producción porque
   sus estados son todos agent-owned. **Verificar con grep antes de borrar, no confiar en esta
   lista** — puede haber cambiado desde que se escribió.
4. Los `*_actions` builders (p. ej. `review_checkout_actions`, `select_payment_method_actions`,
   `main_menu_actions`) probablemente sobreviven más tiempo: varios se comparten entre `transition()`
   (para los estados no agent-owned) y `advisor.rs`. Revisar cada uno individualmente.
5. `transition()` en sí **no se puede borrar completo** — solo se puede achicar su `match` a los
   ~15 estados no agent-owned una vez que los handlers de los estados agent-owned se confirmen
   muertos y se borren. El `match` deja de ser exhaustivo sobre variantes agent-owned; hay que
   decidir si se colapsan con `_ => unreachable!()` o si el enum `ConversationState` se separa en
   dos (estados-agente vs estados-handoff) — esto último es un cambio de diseño más grande, fuera
   de alcance de una limpieza mecánica.

## 3. Flujo de asesor — decisión tomada, ejecución sigue bloqueada

**Decisión (2026-07-31, Samuel):** el bot deja de mandarle WhatsApp al asesor; `crm-app` pasa a ser
la única superficie donde el asesor trabaja el caso.

**Por qué no se ejecuta todavía:** `crm-app/src/server/inbox/send.ts` (envío saliente) sigue
deshabilitado a propósito — sin eso, el asesor no tiene cómo responderle al cliente desde `crm-app`.
Construir ese envío saliente es trabajo en el repo `crm-app`, no en `trabix-bot`, y es prerequisito
antes de tocar `src/bot/states/advisor.rs` (~2.100 líneas) o `relay.rs` (~268 líneas) aquí. Ver
`../crm-app/ROADMAP.md`.

Una vez que `crm-app` tenga envío saliente:
- El bot deja de usar `BindAdvisorSession`/`SendText` hacia el asesor y en su lugar escribe una
  señal de "necesita humano" (probablemente ya cubierto por `message_events` + un campo de estado
  que `crm-app` pueda leer).
- `advisor.rs`/`relay.rs` se reemplazan o se reducen a lo mínimo que siga necesitando el motor de
  agente (si algo).
- Esto también cambia qué estados quedan "no agent-owned" en la sección 1 — varios de los
  `Wait*`/`Negotiate*`/`Relay*` de esa lista podrían desaparecer si el hand-off completo pasa a
  `crm-app`. Re-derivar la lista de la sección 1 cuando esto se ejecute, no reusarla de memoria.

## 4. Migración `005` muerta — resuelto (ver sección 0.3)

## 5. Docs obsoletos

Sin cambios respecto a la nota anterior: `docs/archive/MASTER_PROMPT.md`,
`MASTER_PROMPT_PRODUCCION.md`, `AI_AGENT_FAQ.md`, `todo.md` siguen siendo candidatos a borrar
(ya archivados, no se tocaron en esta sesión).
