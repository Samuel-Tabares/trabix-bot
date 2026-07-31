# ROADMAP — `trabix-bot`

> Léeme al iniciar sesión, junto con `CLAUDE.md`. Este archivo dice **qué sigue y en qué orden**;
> `CLAUDE.md` dice **cómo está construido**. Actualizar este archivo cuando algo se complete.
>
> Última revisión: 2026-07-31.

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

- **Prompt caching** (v1.9.0): `cache_control` en el system prompt estático + tools de
  `src/ai/agent.rs`. Ver `docs/PENDIENTE_prompt_caching.md`.
- **Domicilio gratis Armenia (6–19u) + detal sin mínimo en pueblos aledaños** (v1.9.0):
  `src/bot/delivery_zone.rs`. Ver `docs/PENDIENTE_domicilio_gratis.md`.
- **BOT_ENGINE toggle eliminado**: el bot corre agente siempre; `ANTHROPIC_API_KEY` es obligatoria.
  `reminder_actions()` (dead code real, no solo teórico) y sus helpers de botones huérfanos en
  `timers.rs` también se borraron.
- **`send_quick_replies`/`send_options_list`** conectados como tools del agente (hasta 3 botones /
  10 filas — límites de WhatsApp), sobre los `BotAction::SendButtons`/`SendList` que ya existían.
- **Migración `012`**: `DROP TABLE` de las tablas huérfanas del simulador que creó `005`.
- **CTWA click ID — captura sin credenciales**: `ctwa_clid` se lee del `referral` del webhook y se
  guarda en `customers` (migración `011`, se captura una sola vez, no se sobreescribe). Cliente CAPI
  (`src/capi.rs`) construido y wireado en la confirmación de compra (`UpdateCustomerAndAnalytics` en
  `src/engine.rs`) — hoy es un no-op silencioso porque falta configurar `WABA_ID` /
  `META_CAPI_DATASET_ID` / `META_CAPI_ACCESS_TOKEN`.

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

**Bloqueada de verdad:** tocar `src/bot/states/advisor.rs` (~2.100 líneas) o `relay.rs` (~268)
requiere que `crm-app` tenga envío saliente funcionando primero (`crm-app/src/server/inbox/send.ts`,
hoy deshabilitado a propósito) — si no, el asesor se queda sin forma de responderle al cliente. Ese
trabajo es en el repo `crm-app`, no aquí. Ver `../crm-app/ROADMAP.md`.

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
