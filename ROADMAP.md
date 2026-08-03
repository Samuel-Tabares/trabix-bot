# ROADMAP — `trabix-bot`

> Léeme al iniciar sesión, junto con `CLAUDE.md`. Este archivo dice **qué sigue y en qué orden**;
> `CLAUDE.md` dice **cómo está construido**. Actualizar este archivo cuando algo se complete.
>
> Última revisión: 2026-08-02.

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
  ⚠️ **Pendiente de seguridad:** el endpoint sigue expuesto igual — el dominio público del bot
  enruta `/internal/*` porque el mismo listener sirve `/webhook` (verificado: 401 desde internet).
  Lo único que lo protege es el token. El cierre real es un **segundo listener** (otro puerto, p.ej.
  8081) que sirva solo `/internal/*`: Railway expone un solo puerto al edge público, así que ese
  queda accesible únicamente por la red privada. Cambio chico en `src/main.rs` + `src/routes/mod.rs`.
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
  `general_info/current_runtime_reference.md` → "Envío Nacional". ⚠️ **Pendiente, no de código**:
  confirmar con la transportadora si el retail con licor (12%) se puede despachar nacional sin
  restricción; si no, el canal puede arrancar solo con sin licor (su mínimo de 20u ya coincide).
  Pendiente también sincronizar `website/retail/index.html` (hoy dice "Resto de Quindío, entra a
  mayoristas o alianzas", desactualizado) y el `CLAUDE.md` de la raíz del workspace.
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

---

## Orden de ejecución

### 1. CTWA + Conversions API — falta solo credenciales

**Todo el código está listo** (captura de `ctwa_clid`, migración, cliente CAPI fail-silent, wireado
al momento real de compra). Lo único que falta:

1. **Samuel consigue Dataset ID + access token** desde Meta Business Manager. Flujo (WhatsApp
   Business Messaging, distinto del Pixel de sitio web):
   - Permisos `whatsapp_business_management` + `whatsapp_business_manage_events` en la app de
     desarrollador (Advanced Access).
   - Confirmar "Marketing API Access Tier" (gate de Meta: 1.500 llamadas exitosas en 15 días — puede
     ya estar satisfecho una vez la pauta esté corriendo).
   - `POST https://graph.facebook.com/v21.0/{WABA_ID}/dataset?access_token={TOKEN}` → devuelve
     `dataset_id`. El `WABA_ID` es el mismo que usa la suscripción del webhook.
   - Token: Business Settings → System Users → generar uno con esos dos scopes, sin expiración.
2. Cargar `WABA_ID`, `META_CAPI_DATASET_ID`, `META_CAPI_ACCESS_TOKEN` en Railway.
3. Verificar en la consola de Meta (Events Manager → dataset) que empiezan a llegar eventos
   `Purchase` tras un pedido confirmado real.
4. **Cambiar el evento de optimización** en Meta de "Conversaciones" a "Compras" cuando haya volumen
   (~50 compras/semana).

Detalle completo: `../docs/PENDIENTE_capi_meta.md`.

---

### 2. Limpieza del motor determinístico — decisión tomada, ejecución en dos partes

**Decisión de Samuel (2026-07-31):** el bot deja de mandarle WhatsApp al asesor; `crm-app` pasa a
ser la única superficie de trabajo del asesor.

**Bloqueada de verdad, pero ya con el primer eslabón puesto:** el lado bot del mecanismo existe
desde v1.11.0 (`POST /internal/advisor/send`, ver arriba). Lo que sigue faltando antes de tocar
`src/bot/states/advisor.rs` (~2.100 líneas) o `relay.rs` (~268) es que `crm-app` esté **desplegada**
y que `crm-app/src/server/inbox/send.ts` llame de verdad a ese endpoint, probado punta a punta en
producción — si no, el asesor se queda sin forma de responderle al cliente. Ese trabajo es en el
repo `crm-app`, no aquí. Ver `../crm-app/ROADMAP.md`.

**Resuelto (v1.15.0, Fase 2):** los dos comportamientos que le faltaban al endpoint interno —
(a) silenciar al bot mientras un humano tiene el caso y (b) pausar el timer de inactividad en esa
situación — ya están shippeados vía `conversations.human_takeover_until`. Ver "Ya shippeado" arriba
y `docs/internal_advisor_send.md`. Sigue pendiente el cutover en sí (los dos caminos vivos,
monitorear, después cortar) — eso sigue bloqueado en que `crm-app` termine de probar el envío
saliente punta a punta, no en esto.

**Lo que se puede seguir sacando en este repo sin esperar esa decisión** (borrado ya empezó esta
sesión, ver "Ya shippeado" arriba, pero queda trabajo real): hallazgo clave de esta sesión — el plan
original ("borrar por archivo si `src/ai/` no lo importa") **estaba mal**: `transition()` sigue
siendo código vivo hoy para ~15 estados de hand-off (`WaitAdvisorResponse`, `RelayMode`,
`SelectReferralOption`, etc. — lista completa en el doc de abajo), y varias funciones `*_actions` de
`menu.rs`/`checkout.rs`/`order.rs`/`data_collect.rs`/`scheduling.rs`/`customer_data.rs` las comparten
con `advisor.rs`. El borrado real que queda es **función por función, no archivo por archivo**.

Plan corregido y lista de funciones candidatas (`handle_review_checkout`,
`handle_select_payment_method`, `handle_main_menu`, etc. — los handlers de estados agent-owned, que
sí parecen genuinamente muertos pero faltan verificar con grep antes de borrar):
`docs/CLEANUP_deterministic_engine.md` (reescrito esta sesión con los hallazgos).

---

### 3. Cosas menores sueltas

- Docs obsoletos (`docs/archive/MASTER_PROMPT.md`, etc.) — ya archivados, candidatos a borrar
  cuando alguien pase por ahí. Cero urgencia.

---

## Definición de "listo" para cualquier cambio aquí

`cargo check` + `cargo test` en verde · `CHANGELOG.md` actualizado · versión bumpeada en
`Cargo.toml` · `general_info/current_runtime_reference.md` actualizado si cambió comportamiento de
runtime · commit directo a `master` (no hay flujo de PR en este repo).
