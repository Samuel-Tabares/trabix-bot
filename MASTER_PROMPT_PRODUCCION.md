# MASTER PROMPT: Agente IA en Producción (WhatsApp real, todo el público)

**Fecha:** 2026-07-14
**Versión:** 1.0
**Estado:** 📋 PENDIENTE DE EJECUCIÓN
**Prerequisito:** MASTER_PROMPT.md (FASES 1-8) completado y auditado (ver sección "Audit de verificación independiente" en ese documento).

---

## ⚠️ ADVERTENCIA CRÍTICA

**ANTES DE CUALQUIER CAMBIO, VERIFICA EL ESTADO REAL EN EL CÓDIGO:**

1. `src/config.rs` — gate `AgentEngineRequiresSimulator` (el agente hoy NO puede correr en producción)
2. `src/ai/memory.rs` — la memoria permanente se envía COMPLETA al LLM en cada turno
3. `src/routes/webhook.rs` — el webhook responde `200 OK` y procesa async; errores solo se loguean
4. `src/engine.rs` → `is_agent_owned_state()` — qué estados posee el agente y cuáles siguen deterministas

**Si algo contradice este documento, DETENTE y verifica primero. NUNCA asumas.**

---

## 🎯 OBJETIVO

Pasar de "agente IA funcionando en simulador" a "agente IA atendiendo a todo el público en el
WhatsApp real de Trabix vía Railway", sin regresiones del motor determinista, con costos de LLM
controlados y con degradación segura si Anthropic falla.

**Filosofía de esta fase (decisión de Samuel):** primero que funcione perfecto; optimizaciones y
eliminación de código legado vienen DESPUÉS, en un ciclo separado.

---

## ✅ DECISIÓN YA TOMADA: RELAY

**El sistema YA funciona como se quiere en modo agente.** El agente usa `message_advisor` para
enviar contexto al asesor (quién es el cliente, su número, la consulta) y el asesor contacta al
cliente manualmente por fuera del bot. No hay relay en el flujo del agente.

El relay (`RelayMode`, `WaitAdvisorContact`, `TimerType::RelayInactivity`) solo existe en el motor
determinista legado, donde sigue siendo funcional.

**Regla para esta fase:**
- ❌ NO eliminar el relay ni sus estados/timers/handlers todavía.
- ✅ SÍ verificar (FASE 4) que en modo agente ningún camino puede caer en `relay_mode` ni
  `wait_advisor_contact`. Si existe un camino, redirigirlo a `message_advisor` + `manual_followup`.
- 🗓️ La eliminación del relay queda agendada para el ciclo de optimización posterior, cuando el
  agente lleve tiempo estable en producción y el motor determinista deje de ser el fallback.

---

## 📋 BRECHAS IDENTIFICADAS (por qué el sistema aún no está listo)

| # | Brecha | Archivo | Severidad |
|---|--------|---------|-----------|
| 1 | `BOT_ENGINE=agent` es rechazado con `BOT_MODE=production` (gate explícito) | `src/config.rs` (~línea 95 y 129) | Bloqueante |
| 2 | Historial completo del cliente va al LLM en cada turno: costo crece sin límite por cliente (memoria permanente = tokens permanentes) | `src/ai/memory.rs` → `load_messages()` | Bloqueante (costo) |
| 3 | Si la API de Anthropic falla (timeout, 5xx, saldo agotado), el error solo se loguea: el cliente queda EN SILENCIO sin respuesta ni aviso al asesor | `src/routes/webhook.rs` (~línea 61) + `src/engine.rs` | Bloqueante (UX) |
| 4 | Sin límite de gasto: cualquier desconocido puede escribir mil mensajes y cada uno dispara llamadas al LLM (hasta `MAX_TOOL_ITERATIONS` cada una) | `src/ai/agent.rs`, `src/engine.rs` | Alta |
| 5 | Sin deduplicación por `message_id` de Meta: un retry de Meta puede procesar el mismo mensaje dos veces | `src/routes/webhook.rs` | Media |
| 6 | Instrucciones anti prompt-injection ausentes del system prompt (los precios/cálculos ya son deterministas, pero el tono/promesas del agente son manipulables) | `src/ai/agent.rs` → `SYSTEM_PROMPT` | Media |
| 7 | Smoke test real de WhatsApp nunca corrido en este ciclo (`tests/live_whatsapp.rs` requiere credenciales) | `.env` / Railway | Media |
| 8 | crm-web sin decisión de despliegue (hoy solo corre local) | `crm-web/` | Baja (no bloquea el bot) |

---

## 🚀 PLAN DE IMPLEMENTACIÓN

### FASE 0: Baseline y verificación previa

- [ ] `cargo test` → 145/145 verdes, 0 warnings
- [ ] Tests de BD contra Postgres local desde cero:
      `TEST_DATABASE_URL=... cargo test -- --ignored --test-threads=1`
      (crear la BD VACÍA y dejar que sqlx aplique migraciones; NO aplicar migraciones con psql antes)
- [ ] Leer `src/config.rs`, `src/ai/memory.rs`, `src/routes/webhook.rs`, `src/engine.rs` completos
- [ ] Backup de la BD de Railway antes de tocar nada

### FASE 1: Habilitar el agente en producción

- [ ] Eliminar el gate `AgentEngineRequiresSimulator` de `src/config.rs` (variante del error,
      check en `from_env`, y sus tests en el mismo archivo)
- [ ] Mantener el requisito: `BOT_ENGINE=agent` exige `ANTHROPIC_API_KEY` presente (ya existe)
- [ ] `BOT_ENGINE` sin definir sigue siendo `deterministic` — el default NO cambia (rollback
      instantáneo: quitar la variable en Railway y redeploy)
- [ ] Actualizar CHANGELOG y el comentario del CLAUDE.md del bot si menciona el gate

### FASE 2: Degradación segura si el LLM falla

**Regla:** el cliente NUNCA debe quedar en silencio y el asesor SIEMPRE debe enterarse.

- [ ] En `process_customer_input` / `process_advisor_input` (`src/engine.rs`): si
      `run_customer_turn`/`run_advisor_turn` retorna `Err`:
  - [ ] Enviar al cliente un mensaje fijo desde `config/messages.toml` (nuevo campo), estilo:
        "Estamos teniendo un problema técnico 🙏 Un asesor te contactará en breve."
  - [ ] Enviar al asesor: número del cliente + último mensaje + estado actual del caso
  - [ ] NO cambiar el estado de la conversación (el caso queda donde estaba; el cliente puede
        reintentar y el asesor tiene contexto)
- [ ] Timeout explícito en el cliente HTTP de Anthropic (`src/ai/client.rs`) si no existe —
      verificar; un webhook colgado bloquea el lock de esa conversación
- [ ] Test unitario del camino de error (mock/inyección del fallo)

### FASE 3: Control de costos del LLM

- [ ] **Ventana de memoria:** en `src/ai/memory.rs`, separar "historial CRM completo" (se guarda
      todo, nada cambia en BD) de "contexto que va al LLM" (solo los últimos N mensajes, p. ej.
      30-40, o un presupuesto de caracteres). El bloque `ESTADO ACTUAL DEL CASO` del system prompt
      ya lleva los datos duros del pedido, así que recortar historial viejo no pierde datos del caso.
- [ ] **Límite por mensaje:** truncar el texto entrante a un máximo razonable (p. ej. 1.500
      caracteres) antes de mandarlo al LLM
- [ ] **Límite por cliente:** contador de llamadas LLM por teléfono por día (en `state_data` o
      tabla simple); al exceder (p. ej. 60/día), responder mensaje fijo + avisar al asesor
- [ ] **Kill-switch global:** si `BOT_ENGINE=agent` falla la validación de presupuesto diario
      (variable `AGENT_DAILY_LLM_CALL_LIMIT`, opcional), degradar a mensaje fijo + asesor
- [ ] Medir en el canario (FASE 7) el costo real promedio por conversación completa y por pedido
      confirmado; anotar los números en la sesión de cierre

### FASE 4: Hardening de seguridad y consistencia

- [ ] **Dedup de webhook:** ignorar `message_id` ya procesados (cache en memoria con TTL corto es
      suficiente; Meta reintenta en ventanas de minutos)
- [ ] **Anti prompt-injection** en `SYSTEM_PROMPT`: regla explícita de que instrucciones del
      cliente NUNCA cambian precios, descuentos, zonas ni reglas del negocio; ante intentos de
      manipulación, redirigir al pedido o al asesor. (Los cálculos ya son deterministas — esto
      protege tono y promesas.)
- [ ] **Verificar guards de actor** (ya existen — confirmar que TODOS los tools sensibles los
      tienen): `confirm_advisor_availability`, `set_manual_delivery_cost` (solo asesor);
      `set_payment_method` (solo cliente)
- [ ] **Auditar alcanzabilidad del relay en modo agente:** con `BOT_ENGINE=agent`, mapear todas
      las transiciones que salen de estados NO poseídos por el agente (`negotiate_hour`,
      `wait_advisor_*`) y confirmar que ninguna entra a `relay_mode`/`wait_advisor_contact`.
      Si alguna entra → redirigir a `message_advisor` + `manual_followup`. Documentar el resultado.
- [ ] `ADVISOR_PHONE` ≠ `WHATSAPP_TEST_RECIPIENT` en cualquier prueba en vivo (regla ya conocida)

### FASE 5: Pruebas completas

- [ ] `cargo test` + tests ignorados de BD (como FASE 0)
- [ ] Simulador (`BOT_MODE=simulator BOT_ENGINE=agent`): repetir los flujos de FASE 7 del ciclo
      anterior + los NUEVOS caminos: error de LLM simulado, cliente que excede límite diario,
      mensaje gigante truncado, código referral aplicado DESPUÉS de la confirmación del asesor
      (verificar que analytics lo captura — fix del audit 2026-07-13)
- [ ] Smoke real: `cargo test --test live_whatsapp -- --ignored --test-threads=1` con credenciales
      reales en `.env`

### FASE 6: Checklist Meta (una sola vez, verificar aunque ya esté hecho)

- [ ] App de Meta en modo **Live**
- [ ] WABA suscrita a la app: `GET /{WABA_ID}/subscribed_apps` debe listar la app
- [ ] Webhook apuntando a `https://<railway-domain>/webhook` con el verify token correcto
- [ ] Token permanente (system user token), no token temporal de 24h
- [ ] Número de producción con display name aprobado; revisar límites de mensajería del número
      (tier inicial de Meta limita conversaciones/día — subir tier requiere volumen + calidad)
- [ ] `MENU_IMAGE_MEDIA_ID` vigente (los media_id de Meta expiran; re-subir con
      `cargo run --bin upload_media` si hace falta)

### FASE 7: Deploy canario en Railway

Variables en Railway para el corte a agente:

| Variable | Valor | Nota |
|----------|-------|------|
| `BOT_MODE` | *(sin definir)* | production es el default |
| `BOT_ENGINE` | `agent` | quitar la variable = rollback a determinista |
| `ANTHROPIC_API_KEY` | *(secreto)* | requerido por el agente |
| `ADVISOR_PHONE` | número real del asesor | ≠ número de pruebas |
| `DATABASE_URL`, `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_NUMBER_ID`, verify token | ya existentes | sin cambios |
| `FORCE_BOGOTA_NOW` | **NUNCA en Railway** | solo testing local |

Plan de corte:

- [ ] Deploy con `BOT_ENGINE` **sin definir** primero → confirmar que el determinista sigue
      intacto en producción (migraciones 007-009 corren solas al boot vía `sqlx::migrate!`)
- [ ] Activar `BOT_ENGINE=agent` → redeploy
- [ ] **Canario (2-3 días):** solo Samuel + 2-3 conocidos ordenan de verdad; el asesor opera normal
- [ ] Revisar cada día: transcripciones (`agent_case_messages` / crm-web), gasto en la consola de
      Anthropic, `orders` confirmadas vs. conversaciones iniciadas, logs de Railway
- [ ] Criterio de promoción: 0 silencios, 0 totales incorrectos, costo/conversación aceptable
- [ ] Abrir al público (publicar el número / activar campañas)
- [ ] **Rollback documentado:** quitar `BOT_ENGINE` en Railway + redeploy = vuelve el determinista
      con los mismos datos (las tablas son compartidas; nada que migrar de vuelta)

### FASE 8: Observabilidad y operación

- [ ] Log estructurado ya existe (`src/logging.rs`) — añadir contadores mínimos por día si es
      trivial: llamadas LLM, errores LLM, pedidos confirmados
- [ ] Alerta práctica: todo error de agente ya notifica al asesor (FASE 2) — el asesor ES la alerta
- [ ] Decidir despliegue de crm-web (opciones: servicio Railway con la misma `DATABASE_URL` y
      Basic Auth delante, o seguir local-only). SESSION-012 dejó pendiente el login — NO exponer
      público sin autenticación
- [ ] Documentar runbook corto en README o `general_info/`: cómo hacer rollback, cómo leer un caso
      atascado, cómo re-subir el media del menú

### FASE 9: Cierre

- [ ] `CHANGELOG.md`: release `vX.Y.Z` (hay `Added`/`Fixed` acumulados en Unreleased → minor bump)
- [ ] `Cargo.toml` + `Cargo.lock` + tag `git tag -a vX.Y.Z`
- [ ] Actualizar `general_info/current_runtime_reference.md` y diagramas si cambió el runtime
- [ ] SESSION-XXX.md del ciclo

---

## ⚠️ RIESGOS

1. **Costo LLM descontrolado** — mitigado por FASE 3 (ventana de memoria + límites + kill-switch);
   verificar números reales en canario ANTES de abrir al público.
2. **Anthropic caído = bot mudo** — mitigado por FASE 2 (mensaje fijo + aviso al asesor). El
   rollback total (quitar `BOT_ENGINE`) devuelve el determinista completo.
3. **Comportamiento del LLM con público real** (tono, alucinación de promesas) — precios y
   cálculos son deterministas; el canario existe para cazar lo demás. No abrir al público sin
   leer transcripciones reales.
4. **Límites de tier de Meta** — el número nuevo tiene tope de conversaciones/día; crecer requiere
   calidad de mensajería sostenida.
5. **Migraciones en Railway** — corren solas al boot; NUNCA editar una migración aplicada
   (checksum `VersionMismatch` tumba el arranque). Backup antes del primer deploy de este ciclo.

---

## 🎯 CRITERIOS DE ÉXITO

1. ✅ Un cliente real desconocido completa un pedido (detal y mayorista con referral) por WhatsApp
   real sin intervención del asesor salvo confirmar disponibilidad
2. ✅ Caída simulada del LLM → cliente recibe mensaje fijo y asesor recibe contexto (nunca silencio)
3. ✅ Costo por conversación medido y aceptado por Samuel antes de abrir al público
4. ✅ `customers` y `referral_code_analytics` reflejan exactamente los pedidos confirmados del canario
5. ✅ Rollback probado una vez en canario (quitar `BOT_ENGINE` → determinista funciona)
6. ✅ En modo agente no existe camino alcanzable a `relay_mode` (verificado y documentado)
7. ✅ 100% tests verdes, 0 warnings, CHANGELOG y docs alineados

---

## 🚫 FUERA DE ALCANCE DE ESTE CICLO (ciclo de optimización posterior)

- Eliminar relay, `WaitAdvisorContact`, `TimerType::RelayInactivity` y el motor determinista legado
- Migrar `agent_case_messages` de JSONB único a filas por mensaje (escala >5k mensajes/cliente)
- Índices adicionales de BD identificados en SESSION-011
- Botones/listas dinámicos generados por el agente
- Dashboard de analytics de referral program-wide en crm-web

---

## 📞 DECISIONES QUE REQUIEREN A SAMUEL

1. Presupuesto diario de LLM aceptable (para el kill-switch de FASE 3)
2. Números del canario (quiénes prueban) y duración (sugerido: 2-3 días)
3. crm-web: ¿Railway con Basic Auth o sigue local?
4. Cuándo cortar: el deploy con `BOT_ENGINE=agent` es reversible en minutos, pero el primer día
   conviene hacerlo con el asesor disponible

---

**Creado:** 2026-07-14
**Por:** Claude Fable 5 (a partir del audit de verificación 2026-07-13)
**Estado:** 📋 Listo para ejecutar — empezar por FASE 0
