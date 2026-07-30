# ROADMAP — `trabix-bot`

> Léeme al iniciar sesión, junto con `CLAUDE.md`. Este archivo dice **qué sigue y en qué orden**;
> `CLAUDE.md` dice **cómo está construido**. Actualizar este archivo cuando algo se complete.
>
> Última revisión: 2026-07-30.

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

## Orden de ejecución

Los ítems 1–3 son **independientes entre sí y no dependen de nadie**. El 4 depende de una decisión
que se toma en `crm-app`. El 5 es mecánico.

### 1. Prompt caching — máxima prioridad

**Por qué primero:** es lo único de toda la lista que se paga solo, no depende de terceros, y se
hace en una sesión. Ahorro estimado **$300.000–350.000 COP por ciclo de pauta** — el doble de lo que
se ahorraría renegociando la tasa del inversionista.

**Estado actual (verificado):** `cache_control` no aparece en `src/ai/`. El system prompt
(~11.400 caracteres, `src/ai/agent.rs`) y ~10 definiciones de tools se reenvían completos, a precio
completo, **en cada turno**. Con ~15 llamadas por conversación que compra, se paga el mismo bloque
quince veces. Costo hoy: ~$1.650 COP por conversación que compra, ~$550 por la que no —
**~$460.000 por ciclo de 640 conversaciones**, ~28% de lo que se gasta en pauta.

Esto es un **costo variable que escala linealmente con la inversión en anuncios**, y se paga en
*todas* las conversaciones, no solo en las que compran.

**Qué hacer:**
1. Marcar cacheable el bloque estático: system prompt + definiciones de tools.
2. Respetar el orden: lo estático primero, lo dinámico después. El bloque **"ESTADO ACTUAL DEL
   CASO"** cambia cada turno → va **fuera** del segmento cacheado.
3. Medir antes/después: gasto de la consola de Anthropic ÷ conversaciones del periodo.

**Verificar al implementar:** el contrato exacto de caching en la API de Anthropic vigente (usar la
skill `claude-api`, no memoria). El estimado de costo de arriba puede errar por 3× — contrastar con
la consola real.

**Después, no ahora:** evaluar Haiku 4.5 para turnos rutinarios reservando Sonnet para los de
criterio comercial. Solo con caching ya medido y datos de conversión que permitan comparar.

Detalle completo: `../docs/PENDIENTE_prompt_caching.md`.

---

### 2. Domicilio gratis Armenia + detal en pueblos aledaños

**Aprobado por Samuel el 2026-07-30, sin implementar.** Es **prerequisito de lanzar la pauta**: los
creativos y el plan de ads ya prometen *"6 por $36.000 con domicilio gratis"*, y el bot todavía
cobra domicilio en esos pedidos. Lanzar antes de esto = anunciar algo que el bot no cumple.

**Regla exacta a implementar:**

| Cantidad | Zona | Domicilio |
|---|---|---|
| 1–5 u | Armenia | Tarifa de zona (norte $6.000 / centro $8.000 / sur $10.000) |
| **6–19 u** | **Armenia** | **GRATIS** |
| 20+ u | Armenia | Precio mayorista + domicilio cobrado |
| **Cualquiera** | **Grupo A** | Tarifa cobrada, **sin mínimo de unidades** |
| 20+ u | Grupo B | Tarifa cobrada, **mínimo 20 u se mantiene** |

**Grupo A** (se elimina el mínimo): Calarcá $15.000 · El Caimo $15.000 · Circasia $16.000 ·
Montenegro $16.000 · La Tebaida $16.000 · Pueblo Tapao $20.000 · Barcelona $21.000.

**Grupo B** (mínimo 20 u intacto): Quimbaya $32.000 · Salento $40.000 · Filandia $45.000 ·
Buenavista $45.000 · Pijao $45.000 · Córdoba $48.000 · Génova $48.000.

**El domicilio gratis es exclusivo de Armenia.** En todos los pueblos siempre se cobra tarifa.

**Por qué el Grupo B conserva el mínimo:** no es margen, es **costo de oportunidad operativo** — un
viaje a Génova bloquea al domiciliario más de una hora mientras los pedidos de Armenia esperan.

**Por qué no hay riesgo financiero en quitar el mínimo del Grupo A:** el cliente paga el domicilio.
Un pedido con mala relación precio/domicilio simplemente no convierte; no genera pérdida.

**Notas técnicas obligatorias:**
- Es **código determinista** en `src/bot/delivery_zone.rs` (hoy `MIN_UNITS_OUTSIDE_ARMENIA = 20`)
  más las tools. **No se resuelve escribiéndolo en el system prompt.**
- El guard anti-alucinación bloquea cualquier cifra `$X.XXX` que no venga textual en un tool-result
  → la tool de cálculo **debe devolver el domicilio en $0 explícitamente** para que el agente lo
  pueda comunicar.
- `checkout::render_summary` ya distingue "domicilio desconocido" (`None`) de "conocido y vale $0"
  (`Some(0)`) — ese caso ya está contemplado.
- Actualizar `config/messages.toml`. El mensaje de mayor impacto en ticket promedio es
  **"te faltan N unidades para domicilio gratis"**.

**Verificación de que no se abre un hueco de precio:** 19 u al detal = $116.000 con domicilio gratis;
20 u mayorista = $98.000 + domicilio. El mayorista sigue conviniéndole al cliente. Correcto.

Detalle completo: `../docs/PENDIENTE_domicilio_gratis.md`.

---

### 3. CTWA click ID + Conversions API de Meta

**Aprobado, sin implementar.** Es el cambio de mayor impacto sobre el **rendimiento** de la pauta.

**El problema:** Meta no ve nada de lo que pasa dentro de WhatsApp. Un anuncio click-to-WhatsApp
optimiza por defecto hacia *"conversaciones iniciadas"*, no hacia compras — el algoritmo busca gente
barata que abra chats, no compradores. Resultado típico: muchas conversaciones, pocos pedidos, y la
conclusión equivocada de que "la pauta no sirve". **Sin esto, escalar presupuesto solo escala el
desperdicio.**

**Qué hacer:**
1. **Capturar el click ID.** El webhook de Meta trae un objeto `referral` en el primer mensaje de
   quien llega por un anuncio, con un identificador de clic (`ctwa_clid`). Leerlo en
   `src/routes/webhook.rs` y guardarlo en `customers` (columna nueva, nullable).
2. **Reportar la compra.** Cuando `finalize_checkout` confirma un pedido, enviar un evento
   `Purchase` a la Conversions API con ese click ID, el valor real de la venta y `COP`.
3. **Cambiar el evento de optimización** en Meta de "Conversaciones" a "Compras" cuando haya volumen
   (~50 compras/semana).

**Requisitos duros:**
- **Dataset ID + access token** desde Meta Business Manager — **los tiene que conseguir Samuel**,
  no se puede avanzar sin eso.
- Migración append-only (la última es `010_create_message_events.sql` → la siguiente es `011`).
- El cliente HTTP hacia la CAPI **debe fallar en silencio y loguear**. Nunca puede tumbar ni demorar
  la confirmación de un pedido — es telemetría, no ruta crítica.
- Deduplicación por `event_id` para que un reintento no reporte la venta dos veces.

**Verificar al implementar:** el nombre exacto del campo y el schema del evento en la documentación
vigente de Meta. La mecánica es estable, el schema cambia.

Detalle completo: `../docs/PENDIENTE_capi_meta.md`.

---

### 4. Limpieza del motor determinístico — BLOQUEADA por una decisión

**No arrancar sin resolver primero la pregunta del asesor.** La decisión se toma en `crm-app`, no
aquí (ver `../crm-app/ROADMAP.md`): ¿el bot deja de mandarle WhatsApp al asesor y solo escribe una
señal de "necesita humano" que `crm-app` muestra en su bandeja, o `crm-app` es solo una vista de
lectura y el WhatsApp al asesor se mantiene?

`src/bot/states/advisor.rs` (~2.100 líneas) + `relay.rs` (~268) **no son código legacy**: son la
única vía de escalamiento a humano en producción hoy. Borrarlos sin resolver esa pregunta rompe el
canal.

Lo que **sí** se puede hacer sin esa decisión (~2.700–3.000 líneas muertas): borrar `menu.rs`,
`customer_data.rs`, `transition()` de `state_machine.rs`, los handlers FSM de `checkout.rs`, y el
toggle `BOT_ENGINE` completo. **Antes de borrar cualquier cosa, hacer grep de qué importa
`src/ai/agent.rs` de cada archivo** — no confiar en listas escritas antes de esta sesión.

Plan detallado, incluyendo qué funciones **mantener** de cada archivo mixto:
`docs/CLEANUP_deterministic_engine.md`.

**Ganancia independiente que no espera a nada:** `src/whatsapp/buttons.rs` ya tiene
`send_buttons`/`send_list` implementados pero marcados dead-code — `src/ai/agent.rs` nunca los llama.
Conectarlos como tools del agente (botones/listas interactivas) es aditivo y de bajo riesgo.

---

### 5. Cortar release — mecánico

`CHANGELOG.md` tiene ~50 líneas en `[Unreleased]` (sabor tamarindo, regla sin licor al detal,
mínimo 24h en programados, upgrade de modelo, `message_events`) mientras `Cargo.toml` sigue en
`1.8.0`. Hay una release sin cortar. Bump + tag `vX.Y.Z` según las convenciones de `CLAUDE.md`.

**Migración muerta:** `migrations/005_create_simulator_tables.sql` creó tablas del simulador
(removido en v1.8.0) que ya no referencia nadie. **No editar esa migración** (append-only) — crear
una nueva que haga `DROP TABLE`.

---

## Definición de "listo" para cualquier cambio aquí

`cargo check` + `cargo test` en verde · `CHANGELOG.md` actualizado · versión bumpeada en
`Cargo.toml` · `general_info/current_runtime_reference.md` actualizado si cambió comportamiento de
runtime · commit directo a `master` (no hay flujo de PR en este repo).
