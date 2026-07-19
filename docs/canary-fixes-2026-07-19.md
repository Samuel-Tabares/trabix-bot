# Canary fixes — 2026-07-19

Backlog de correcciones del motor agente (`BOT_ENGINE=agent`, live en producción desde
2026-07-15). Fuente: pruebas reales de Samuel desde otro celular como cliente + revisión de
logs de Railway del 2026-07-19 (~07:00–07:45 AM Bogotá / 12:00–12:45Z).

**Estado (sesión de fixes 2026-07-19, ver SESSION-016):** ítems **2, 8, 4, 5** y el hallazgo
**D** quedaron resueltos y con tests (`cargo test`: 177 passed, 0 failed). Todavía sin
commitear. Pendientes para la próxima sesión: ítems **1, 3, 6, 7, 9**, hallazgo **A** y **C**
(marcados `⏳ PENDIENTE` abajo). En varios ítems resueltos, la causa real terminó siendo
distinta de la hipótesis original de este documento — cada sección resuelta trae una nota
"Resuelto" con la causa real encontrada.

Contexto técnico para la sesión que corrija:

- El agente vive en `src/ai/` (`agent.rs` system prompt + loop, `tools.rs` tools
  deterministas, `budget.rs`, `memory.rs`, `client.rs`).
- El pricing determinista (`src/bot/pricing.rs`) está **correcto** — verificado: promo por
  pares con licor = `(pares × $12.000) + (impares × $8.000)` (línea 64), 7 unidades =
  $44.000. Los errores de totales son del LLM narrando números que no salieron de una tool
  (misma clase de bug "narrated-not-called" ya corregida 2 veces antes, ver SESSION-014).
- Tools existentes hoy: `get_menu`, `check_business_hours`, `add_order_item`,
  `remove_order_item`, `restart_order`, `get_order_summary`, `set_customer_field`,
  `set_delivery_immediate`, `set_delivery_scheduled`, `set_delivery_nearby_town`,
  `lookup_nearby_town`, `apply_referral_code`, `set_payment_method`, `finalize_checkout`,
  `cancel_order`, `message_advisor`, `message_customer`, `confirm_advisor_availability`.

---

## 1. No respeta el horario de atención (reportado) — ⏳ PENDIENTE

**Síntoma:** a las ~7:00 AM el bot dijo que SÍ había domicilios disponibles. Solo al
volverle a preguntar explícitamente rectificó el horario real (8:00 AM – 11:00 PM).

**Evidencia:** log 12:25Z (7:25 AM Bogotá) — el bot sí sabe rectificar: "en este momento
estamos fuera del horario de entrega inmediata". El mensaje incorrecto anterior ocurrió
antes de la ventana de logs capturada; reportado directamente por Samuel. Además a las
12:39Z el bot confirmó un pedido inmediato "PARA HOY 8:00 AM" estando a las 7:39 AM.

**Causa probable:** `check_business_hours` es una tool que el LLM llama *si quiere*. Si no
la llama, responde de memoria.

**Fix propuesto:** inyectar el estado de horario (abierto/cerrado + hora Bogotá actual) en
el system prompt en **cada turno** (dato determinista, no opcional), en vez de depender de
que el LLM decida llamar la tool. Mantener la tool para preguntas explícitas.

## 2. Totales mal calculados — LLM hace aritmética propia (reportado, CRÍTICO) — ✅ RESUELTO

**Síntoma:** 7 Smirnoff → el bot dijo $40.000; el real es $44.000 (3 promos por pares =
$36.000 + 1 unidad $8.000). El bot afirma que "ese número lo saca el sistema automático" —
falso: el sistema determinista calcula bien; el LLM narra totales sin llamar
`get_order_summary`.

**Evidencia en logs (12:29Z):** cliente pidió "190 Smirnoff de lulo y 10 manzana, ¿cuánto
sería el total?" → el LLM llamó `add_order_item` (solo el smirnoff) e inmediatamente
respondió un "RESUMEN" con total $925.000 narrado por él mismo, sin `get_order_summary` de
por medio. Después el total real en DB fue $900.000 subtotal + $32.000 domicilio = $932.000.

**Fix propuesto (centrarse aquí — cálculo interno del sistema):**
- `add_order_item` / `remove_order_item` deben devolver en su tool-result el resumen
  completo recalculado (items, subtotales por bucket, total) para que el LLM siempre tenga
  las cifras correctas sin llamada extra.
- Regla dura en system prompt: prohibido enunciar cualquier cifra (unitario, subtotal,
  total, domicilio) que no venga textual de un tool-result del turno actual.
- Considerar un validador post-turno: si el texto saliente contiene montos `$X` que no
  aparecen en ningún tool-result reciente, bloquear/reintentar (guard determinista, no
  confianza en el prompt).

> **Resuelto (SESSION-016):** se verificó `pricing.rs` con 6 escenarios reales (7 Smirnoff,
> mayorista, mixto, con domicilio) — el cálculo ya era correcto en todos los casos; el bug
> era 100% narración del LLM (a veces sin haber llamado `add_order_item` para el ítem que
> mencionaba). `add_order_item` ya devolvía el resumen recalculado (no hacía falta tocarlo).
> Se agregó un guard post-turno (`extract_currency_amounts`/`known_tool_amounts`/
> `sanitize_hallucinated_amounts` en `agent.rs`) que bloquea cualquier `$` mencionado que no
> venga de un tool-result real de la conversación. Hallazgo aparte: existe una "super-tool"
> `calculate_order_with_delivery` en `tools.rs` que nunca se usa (código muerto, no conectada
> en `dispatch_tool`) — reportado, no eliminado, decisión pendiente de Samuel.

## 3. Quitar todos los botones — todo por LLM (nuevo comportamiento) — ⏳ PENDIENTE

**Spec (confirmado por Samuel):** al primer mensaje el bot manda un mensaje genérico de
bienvenida **sin botones** — texto plano que lista las opciones de lo que la persona puede
hacer (pedir, ver menú, hablar con asesor, etc.). Ese es el único mensaje "fijo" del flujo.
De ahí en adelante todo es conversación natural 100% LLM, **cero botones/listas
interactivas de WhatsApp en todo el flujo**. Los timers se mantienen igual.

Nota de implementación: eliminar todos los puntos donde el runtime agente aún emite
botones/listas heredados (saludo `main_menu`, selector de pago, referral, etc.) y
convertirlos a texto conversacional.

## 4. Transferencia + pedido programado roto (reportado, CRÍTICO) — ✅ RESUELTO

**Síntomas:**
1. Con pago por transferencia el bot dice que "primero debe confirmar la entrega". Regla de
   negocio: **un pedido programado nunca se confirma con el asesor — se autoacepta**, solo
   se le dice al cliente que quedó programado.
2. Al "enviar los datos de transferencia" no envía nada real — llega algo como `[]` vacío.
3. No reenvía el comprobante recibido al asesor.

**Evidencia en logs:** 12:36Z — `confirm_advisor_availability` fue llamada para un pedido
programado "mañana 8 AM" (violación de la regla de autoaceptación). 12:20Z — "recibí tu
comprobante de transferencia 👍 Tu pedido está 100% confirmado" al cliente, sin ningún
reenvío de media al asesor en los logs.

**Flujo correcto (spec):** programado + transferencia →
1. Confirmación final del pedido con el cliente (ver ítem 7).
2. Enviar los datos reales de transferencia (revisar que el texto exista en
   `config/messages.toml` / no esté llegando vacío — ahí está el `[]`).
3. Esperar imagen del comprobante (timer 10 min ya existe).
4. Al recibirla: confirmar el pedido al cliente y **reenviar el comprobante al asesor**
   para que él verifique personalmente.

> **Resuelto (SESSION-016):** causa real — `finalize_checkout` mandaba TODOS los pedidos
> (inmediatos y programados) por el mismo camino (`AskDeliveryCost` →
> `confirm_advisor_availability`), violando la regla de autoaceptación. `finalize_checkout`
> ahora bifurca por `delivery_type`: programados con domicilio conocido se autoaceptan de
> inmediato (`auto_accept_scheduled_order`), y `confirm_advisor_availability` ahora **rechaza**
> cualquier llamada sobre un pedido programado (guard determinista, arregla hallazgo D).
> `set_manual_delivery_cost` también autoacepta cuando el asesor da el costo después. El texto
> de transferencia en `config/messages.toml` se verificó completo (no era bug de config); se
> reforzó el prompt para que el LLM nunca narre sus propios datos bancarios (posible causa del
> `[]`). `try_handle_receipt_shortcut` ahora también dispara por contexto
> (`payment_method=transfer` + sin comprobante), no solo por estado exacto, para no perder el
> reenvío ante un desface.

## 5. Catálogo con/sin licor hardcodeado por sabor (reportado) — ✅ RESUELTO

**Síntoma:** Smirnoff de lulo solo existe CON licor, pero si el cliente pide "Smirnoff lulo
sin licor" el bot lo acepta.

**Evidencia en logs:** 12:22Z — "Dale, confirmo: **25 Smirnoff de lulo sin licor**…".
También 12:29Z: cliente dijo "10 manzana" y el bot asignó "Manzana verde Tequila (con
licor)" sin preguntar.

**Fix (spec, refinada por Samuel):**
- **Cada producto es una entidad única** del catálogo, hardcodeada con su característica
  con/sin licor. "Manzana verde" (sin licor) y "Manzana verde Tequila" (con licor) son
  productos DISTINTOS, no variantes de un mismo sabor.
- Cada producto tiene **aliases** de lenguaje natural que resuelven a él: "manzana verde
  tequila" = "manzana con licor" = "manzana verde con licor" → Manzana verde Tequila;
  "manzana verde" a secas → el producto sin licor.
- `add_order_item` debe **rechazar** combinaciones inexistentes (ej. smirnoff_lulo +
  `has_liquor=false`) y devolver error al LLM para que corrija con el cliente.
- El bot solo pregunta al cliente cuando la frase no resuelve de forma única a un producto
  (ej. "manzana" a secas). Si resuelve único, se agrega directo sin inventar variantes.

> **Resuelto (SESSION-016):** el catálogo ya estaba bien modelado (ids separados por mapa
> con/sin licor en `config/messages.toml`) — combos inexistentes ya se rechazaban. El hueco
> real era desambiguación: 4 nombres base (Maracumango, Manzana verde, Bonbonbum con 3
> variantes, Blueberry) existen en más de una variante y nada obligaba a preguntar. Nueva
> tabla determinista `AMBIGUOUS_GROUPS` + `check_flavor_disambiguation` en `tools.rs`;
> `add_order_item` ahora exige un parámetro `customer_wording` (frase literal del cliente) y
> rechaza el intento si es ambiguo y esa frase no distingue la variante. Sabores inequívocos
> (Uva Vodka, Smirnoff de lulo) no requieren palabra extra.

## 6. Formato WhatsApp: un solo asterisco + listas (reportado) — ⏳ PENDIENTE

- Hoy el bot escribe `**negrilla**` (se ve literal en WhatsApp). Debe usar `*negrilla*`
  (un asterisco = negrilla en WhatsApp). Evidencia: prácticamente todos los mensajes en
  logs usan `**...**`.
- Preferir listas para resúmenes/pedidos, más organizado.
- Fix: regla de formato en el system prompt + idealmente post-proceso determinista que
  convierta `**x**` → `*x*` antes de enviar (no confiar solo en el prompt).

## 7. Confusión de fechas en programados + confirmación final obligatoria (reportado) — ⏳ PENDIENTE

**Síntoma:** a veces confunde fechas de pedidos programados (reporte de Samuel; el cambio
"mañana 8 AM" → "HOY 8 AM" visto en logs a las 12:29Z/12:38Z NO es evidencia de bug — fue
Samuel corrigiendo manualmente la fecha).

**Spec nueva:** antes de confirmar **cualquier** pedido, el bot debe recapitular con el
cliente y esperar su OK explícito:
- totalidad de productos (sabor, variante, cantidad),
- fecha y hora de entrega (si es programado),
- dirección,
- total con domicilio incluido.

Si el pago es transferencia: el pedido se confirma automático tras el comprobante, pero la
recapitulación igual va **antes** de mandar los datos de transferencia.

## 8. El domicilio debe calcularse junto con el total de productos (reportado) — ✅ RESUELTO

**Síntoma:** el bot da totales de productos sin domicilio y el costo de envío aparece
después (hoy lo digita el asesor en `ask_delivery_cost`).

**Spec:** el cálculo del domicilio debe ocurrir al mismo tiempo que el cálculo del total de
productos; todo total mostrado al cliente incluye el valor del domicilio. Ya existe
`lookup_nearby_town` / `set_delivery_nearby_town` con costos por zona — usar ese catálogo
para cotizar el domicilio de una vez (el asesor solo interviene si la zona no está en el
catálogo).

> **Resuelto (SESSION-016):** causa real — `context.delivery_cost` es `Option<i32>` pero
> `render_summary` trataba "aún no se conoce" (`None`) igual que "se conoce y vale $0"
> (`Some(0)`), mostrando `Total: $X` (solo el subtotal) como si fuera el total final.
> `render_summary` ahora distingue ambos casos: domicilio pendiente → "Subtotal de productos
> (sin domicilio aún, no es el total final)"; domicilio conocido → "Total" real. Placeholder
> `{total}` → `{total_line}` en `config/messages.toml` y validador en `messages.rs`. Prompt
> reforzado para preguntar la zona antes de cotizar.

## 9. Pedidos al por mayor: preguntar y aplicar código de descuento (reportado) — ⏳ PENDIENTE

**Síntoma:** en pedidos con bucket mayorista el bot no pregunta por código de referido.

**Evidencia en logs:** 12:37Z — pedido de 200 unidades confirmado con
`has_referral_code=false` sin que se preguntara nunca por código.

**Spec:** si el pedido tiene al menos un bucket al por mayor, el bot **siempre** pregunta si
el cliente tiene código de descuento, lo valida contra `config/referrals.toml`
(`apply_referral_code` ya existe) y aplica el descuento al bucket mayorista. Forzarlo como
paso obligatorio del checkout (guard determinista en `finalize_checkout`, no cortesía del
LLM: si hay bucket mayor y no se preguntó, bloquear la confirmación).

---

## Hallazgos adicionales en logs (no reportados por Samuel)

### A. Pedido duplicado en DB al ajustar un pedido ya confirmado (CRÍTICO) — ⏳ PENDIENTE

Secuencia 12:37–12:39Z: la orden **31** quedó `confirmed` ($957.000) → el cliente ajustó la
variante de un item → el agente creó la orden **32** y también la confirmó ($932.000). Dos
órdenes confirmadas en DB para un solo pedido real, y analytics registró
`total_spent_cop=957000` de la orden vieja. Fix: al ajustar un pedido confirmado en la misma
conversación, cancelar/reemplazar la orden previa, no crear otra; corregir analytics.

### B. Presupuesto LLM agotado — NO es bug, sin acción

Los dos "LLM daily budget exhausted" (12:22Z y 12:42Z) fueron porque Samuel probó con 2
celulares el mismo día. Comportamiento esperado del guard. No requiere cambio por ahora.

### C. Datos del cliente: separar datos de Meta vs. personalizados (spec de Samuel) — ⏳ PENDIENTE

El agente aceptó "2222222222" como teléfono del cliente y se lo pasó al asesor (12:41Z).
Decisión de Samuel — en vez de validar el input:

- El sistema guarda **directamente** el nombre y el celular que entrega Meta por webhook
  (`contacts[].profile.name` y `messages[].from`) como datos base, no editables por el
  cliente.
- Campos aparte: **nombre personalizado** y **celular personalizado**, que el cliente puede
  modificar libremente cuantas veces quiera, **sin validación**.
- El paquete al asesor debería mostrar ambos (base de Meta + personalizado si existe), de
  modo que un dato inventado nunca reemplace el dato real de Meta.
- Implica ajustar `set_customer_field` y probablemente columnas nuevas en `conversations`
  (migración append-only).

### D. `confirm_advisor_availability` usada en pedidos programados — ✅ RESUELTO

Relacionado con el ítem 4: esa tool solo tiene sentido para pedidos inmediatos. Para
programados, el flujo debe autoaceptar sin consultar disponibilidad al asesor (el asesor
solo recibe el paquete informativo final).

> **Resuelto (SESSION-016):** ver nota de resolución del ítem 4 — `confirm_advisor_availability`
> ahora rechaza explícitamente cualquier llamada sobre un pedido con `delivery_type=scheduled`.

---

## Orden sugerido de ataque

1. ✅ Ítem 2 (totales narrados) + ítem 8 (domicilio en el total) — es la clase de bug más
   peligrosa: dinero mal cotizado a clientes reales. **Resuelto.**
2. ✅ Ítem 4 + D (transferencia/programados) — **resuelto.** ⏳ Hallazgo A (órdenes
   duplicadas) sigue pendiente — no se tocó en esta sesión.
3. ✅ Ítem 5 (catálogo hardcodeado) — **resuelto.** ⏳ Ítem 9 (referido obligatorio en
   mayorista) sigue pendiente.
4. ⏳ Todo pendiente: ítem 1 (horario en system prompt), ítem 7 (confirmación final), ítem 3
   (sin botones), ítem 6 (formato), C (campos Meta vs. personalizados).

**Próxima sesión — arrancar por:** hallazgo A (duplicado, integridad de datos) e ítem 9
(referido obligatorio en mayorista, dinero/comisiones), luego el bloque 4 restante.

Tras corregir: validar en simulator (`./scripts/run_simulator.sh` + `BOT_ENGINE=agent`),
correr `cargo check && cargo test`, actualizar `general_info/current_runtime_reference.md`
si cambia comportamiento, y redeploy a Railway. **Nota:** los fixes de esta sesión (2, 8, 4,
D, 5) están hechos y con `cargo test` en verde, pero **todavía no se han commiteado ni
desplegado** — pendiente de decisión de Samuel sobre cuándo commitear/desplegar.
