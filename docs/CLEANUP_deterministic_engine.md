# Limpieza del motor determinístico — guía para la próxima sesión

No ejecutar en la sesión que escribió este documento (2026-07-25, actualizado 2026-07-26) — es la
referencia para cuando se aborde la limpieza. Contexto inmediato: el bot solo va a correr con
`BOT_ENGINE=agent` de ahora en adelante (Samuel confirmó que el motor determinístico no se vuelve a
usar y que no se necesita red de rollback).

## 0. Arquitectura futura del sistema — por qué esto le da forma a la limpieza

Decisión de arquitectura (2026-07-26, no se ejecuta todavía, es la referencia para diseñar la
limpieza y el trabajo posterior): **no se migra código de `trabix-bot` hacia `crm-app`**.
`accountability_app/`, `website/` y `trabix-bot/` siguen siendo proyectos independientes, cada uno
con su stack y propósito propios — pero los tres quedan **interconectados a través de `crm-app/`**,
que se convierte en el hub central operativo (absorbiendo eventualmente lo financiero/producción de
`accountability_app`, ver `crm-app/CLAUDE.md`).

Eso redefine qué debe ser `trabix-bot` a largo plazo: **literalmente el bot, nada más**. Su trabajo
es hablar con clientes por WhatsApp, capturar datos, y leer/escribir información en la app central
— todo lo que haría una persona operando WhatsApp más campañas de marketing y finanzas, pero vía
integración (tools/API hacia la app central), no duplicando esa lógica de negocio dentro del bot.
Concretamente, a futuro (fuera de esta limpieza, es la dirección de largo plazo):
- El bot deja de ser dueño exclusivo de datos que la app central también necesita (pricing
  versionado, tiers de embajador, etc.) — hoy vive en `src/bot/pricing.rs` y config propia; a futuro
  podría consultarlos vía la app central en vez de mantener su propia copia.
- El flujo de asesor (punto 3 abajo) se resuelve en esta dirección: el humano trabaja desde
  `crm-app` (su bandeja/CRM), no recibiendo WhatsApp aparte — pero el mecanismo concreto (¿el bot
  sigue enviando WhatsApp al asesor además de escribir en `crm-app`, o `crm-app` pasa a ser la única
  superficie?) sigue sin decidir, ver punto 3.

Esta sección es visión, no una tarea de esta limpieza — los puntos 1-6 de abajo (borrado de FSM
muerto) son válidos y ejecutables independientemente de cuándo se aborde esta migración más grande.

## 1. FSM muerto a borrar (~2,700–3,000 líneas)

Antes de borrar cualquier cosa en `checkout.rs`, `order.rs`, `data_collect.rs`, `scheduling.rs`:
**grep primero** qué funciones importa `src/ai/agent.rs` de cada archivo — no asumir la lista de
abajo sigue vigente si el código cambió entre esta nota y la sesión de limpieza.

| Archivo | Líneas | Acción |
|---|---|---|
| `src/bot/states/menu.rs` | 284 | Borrar completo — sin imports desde `src/ai/`. |
| `src/bot/states/customer_data.rs` | 506 | Borrar completo — sin imports desde `src/ai/`. |
| `src/bot/state_machine.rs` | 851 | Borrar `transition()` (FSM de cliente, dead una vez agent-only). **Mantener** `transition_advisor()` — no es legacy, ver punto 3. Auditar el resto del archivo (tipos compartidos como `BotAction` pueden seguir en uso por el motor de agente). |
| `src/bot/states/checkout.rs` | 1,234 | Mixto. Borrar handlers FSM (`handle_review_checkout`, `handle_select_payment_method`, `handle_wait_receipt`, ~600 líneas). **Mantener**: `render_summary`, `render_items`, `current_order_totals`, `snapshot_from_totals`, `order_confirmation_analytics_action` — usados por `src/ai/agent.rs`. |
| `src/bot/states/order.rs` | 676 | Mixto. **Mantener** `flavor_by_id`, `validate_quantity` (usados por `src/ai/tools.rs`) — confirmar si hay más antes de borrar el resto. |
| `src/bot/states/data_collect.rs` | 281 | Mixto. **Mantener** `validate_address`/`validate_name`/`validate_phone` (usados por `src/ai/tools.rs`). |
| `src/bot/states/scheduling.rs` | 506 | Mixto. **Mantener**: `current_bogota_now()` (confirmado en `src/ai/agent.rs` líneas 638, 980, 2556, 2572), `is_within_business_hours`, `immediate_delivery_hours_text` (usados por el agente) — mover a un módulo compartido (ej. `src/bot/scheduling_helpers.rs`) en vez de dejarlos huérfanos dentro de un archivo cuya parte FSM se borra. |

**No tocar** (infraestructura compartida por ambos motores, no es legacy):
`src/bot/pricing.rs` (481), `src/bot/delivery_zone.rs` (151), `src/bot/timers.rs` (1,445),
`src/bot/inactivity.rs` (224).

## 2. Borrar el toggle `BOT_ENGINE`

- `src/config.rs`: enum `BotEngine`, función `from_env()` (línea ~11), variante `Deterministic` —
  colapsar a un solo modo (agente), sin rama determinística.
- `src/engine.rs:922`: chequeo `state.config.bot_engine.is_agent()` — eliminar la bifurcación,
  dejar el único camino.
- `trabix-bot/.env.example`: quitar el bloque de comentario sobre `BOT_ENGINE=deterministic` /
  rollback instantáneo (ya no aplica).
- `general_info/current_runtime_reference.md`: actualizar cualquier sección que documente el
  toggle o el motor determinístico como opción activa.

## 3. Flujo de asesor — dirección decidida, mecanismo concreto NO es borrado simple

`src/bot/states/advisor.rs` (2,100 líneas) + `src/bot/states/relay.rs` (268 líneas) = 2,368
líneas. **No es código legacy** — es la única vía de escalamiento a humano hoy, para ambos
motores (`is_agent_owned_state()` en `src/engine.rs:888` excluye explícitamente estos estados, así
que siempre corren por el FSM determinístico sin importar `BOT_ENGINE`).

Dirección ya decidida (ver punto 0): el asesor trabaja desde `crm-app` en vez de recibir WhatsApp
aparte. Lo que falta decidir, y por eso este archivo NO se toca todavía, es el mecanismo concreto:
- ¿El bot deja de enviarle WhatsApp al asesor por completo y solo escribe en `message_events` +
  alguna señal de "necesita humano" que `crm-app` muestre en su bandeja? (`crm-app` ya lee
  `message_events` en tiempo casi real — ver `crm-app/src/server/inbox/`.)
- ¿O el bot sigue notificando al asesor por WhatsApp como hoy, y `crm-app` es solo una vista
  adicional de solo lectura, sin reemplazar el canal?
- Si `crm-app` pasa a ser la superficie real de escalamiento, su envío saliente (hoy deshabilitado
  a propósito, `crm-app/src/server/inbox/send.ts`) tiene que resolverse primero — el asesor
  necesita poder responderle al cliente desde ahí.

Decidir esto **antes** de tocar `advisor.rs`/`relay.rs` — si se borra en la misma pasada que el
punto 1 sin resolver esta pregunta, se rompe el único canal de escalamiento humano en producción.

## 4. Migración 005 muerta

`migrations/005_create_simulator_tables.sql` (tablas `simulator_sessions`/`simulator_messages`/
`simulator_media`) — cero referencias en `src/`, sobrante del simulador removido en v1.8.0.
**No editar la migración** (regla de append-only del repo) — crear una nueva migración numerada
que haga `DROP TABLE` de lo que 005 creó.

## 5. Docs obsoletos

- `docs/archive/MASTER_PROMPT.md`, `MASTER_PROMPT_PRODUCCION.md`, `AI_AGENT_FAQ.md`, `todo.md` —
  candidatos a borrar (docs pre-agente, ya superados por `general_info/current_runtime_reference.md`).
- `general_info/current_runtime_reference.md` — revisar el archivo completo (no solo referencias
  puntuales al simulador) por si quedan secciones desalineadas con el estado actual del motor.

## 6. Ganancia independiente — no depende de la limpieza de arriba

`src/whatsapp/buttons.rs` ya tiene `send_buttons`/`send_list` implementados pero marcados
`#![allow(dead_code)]` — `src/ai/agent.rs` nunca los llama; solo el FSM determinístico los usa hoy
(vía `BotAction::SendButtons/SendList` en `src/engine.rs`). Conectarlos como nuevas tools del
agente de IA (botones/listas interactivas en vez de solo texto) es bajo riesgo, aditivo, y se puede
hacer en cualquier momento — no hace falta esperar a esta limpieza ni a la decisión del punto 3.

## Orden recomendado para la sesión de limpieza

1. Resolver la pregunta del punto 3 primero (con Samuel) — determina si `advisor.rs`/`relay.rs` se
   tocan en esta sesión o quedan intactos.
2. Punto 1 (FSM muerto) + punto 2 (toggle `BOT_ENGINE`) — pueden ir juntos, mismo cambio conceptual.
3. Punto 4 (migración 005) y punto 5 (docs) — mecánico, bajo riesgo, en cualquier momento.
4. Punto 6 (botones/listas) — independiente, se puede adelantar incluso antes de esta sesión si hay
   tiempo suelto.

Después de cada cambio: `cargo check` + `cargo test`, y actualizar `CHANGELOG.md` según las
convenciones de commit del repo (`trabix-bot/CLAUDE.md`, sección "Commit conventions").
