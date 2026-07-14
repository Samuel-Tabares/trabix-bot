# Bot de IA Integrado: Preguntas y Respuestas

Documentación de funcionamiento del nuevo Claude Haiku 4.5 AI agent integrado en `trabix-bot`. Última actualización: 2026-07-13.

---

## 1. Mensaje de bienvenida y horario

**¿Al inicio de una conversación siempre manda un mensaje de bienvenida junto con el respectivo horario? De ser así, ¿qué es exactamente lo que dice?**

**Respuesta:** Sí, siempre. El mensaje exacto está definido en `config/messages.toml`:

```
¡Hola! 👋 Bienvenid@ a Trabix Granizados.

Horario de entrega inmediata: 8:00 AM a 11:00 PM

Si nos escribes fuera de ese horario, igual te ayudo a programar tu pedido o a contactar un asesor. Elige una opción para continuar ✨
```

Luego el bot ofrece 2 botones en el menú principal (el botón `Hablar con
Asesor` se eliminó en FASE 4; el agente detecta esa intención por texto libre):
- `Hacer Pedido`
- `Ver Menú`

**Ubicación del código:** `config/messages.toml` (sección `[menu]`)

---

## 2. Envío de imagen del menú

**¿Cuando preguntan el menú, efectivamente envía la imagen del menú?**

**Respuesta:** Sí, siempre. En el estado `view_menu`, el bot envía:

1. Una imagen única usando `MENU_IMAGE_MEDIA_ID`
   - En **producción:** media_id válido de Meta Cloud API
   - En **simulator local:** `assets/trabix-menu.png` (imagen rastreada en el repositorio)
2. Texto de precios y descripción del menú
3. Botones para acciones (`Hacer Pedido`, `Volver al Menú`)

**Ubicación del código:**
- Tool: `src/ai/agent.rs` → `dispatch_tool()` → `"show_menu_image"`
- Mensajes: `config/messages.toml` (sección `[menu]`)

---

## 3. Mostrar precios (detal y al por mayor)

**¿Cuando preguntan los precios, efectivamente muestra los precios al detal y al por mayor? ¿Qué muestra exactamente?**

**Respuesta:** Sí, ambos grupos de precios completos. El texto exacto que aparece en `Ver Menú`:

### DETAL:
- **Con licor:** $8.000
- **Par con licor:** $12.000 (2 unidades, la segunda a mitad de precio)
- **Sin licor:** $7.000 c/u

### AL MAYOR (20+ del mismo tipo):

**Con licor:**
- 20-49 unidades → $4.900 c/u
- 50-99 unidades → $4.700 c/u
- 100+ unidades → $4.500 c/u

**Sin licor:**
- 20-49 unidades → $4.800 c/u
- 50-99 unidades → $4.500 c/u
- 100+ unidades → $4.200 c/u

El bot incluye también un mensaje invitativo: "Si quieres, te acompaño a armar tu pedido paso a paso ✨"

**Ubicación del código:** `config/messages.toml` (sección `[menu]`, campo `menu_text`)

---

## 4. Registro de conversaciones

**¿Hay algún registro de conversaciones? ¿Un lugar donde pueda ver lo que habla el bot con los clientes para hacer seguimiento?**

**Respuesta:** Sí, en dos lugares:

### A. Tabla `agent_case_messages` (Nuevo con AI agent)
- **Qué persiste:** Historial completo de mensajes JSON entre cliente/asesor y Claude Haiku
- **Estructura:** `phone_number` (PK), `messages` (JSONB), `updated_at`
- **Cuándo se limpia:** Nunca (desde FASE 3 el historial es permanente por cliente; es la base del CRM)
- **Cómo acceder:** 
  - Directamente en PostgreSQL: `SELECT * FROM agent_case_messages WHERE phone_number = '+57...'`
  - Desde el simulator: Inspector de BD en `/simulator` → tabla `agent_case_messages`

### B. Tablas transaccionales (Datos del pedido)
- `conversations`: nombre, teléfono, dirección, estado actual de la conversación
- `orders`: items, totales, método de pago, código de referido
- `order_items`: detalles de cada producto agregado

**Ubicación del código:**
- Persistencia: `src/ai/memory.rs` (funciones `load_messages`, `save_messages`, `clear_messages`)
- Migración: `migrations/007_create_agent_case_messages.sql`

---

## 5. Eliminar lógica de agendamiento automático

**Eliminemos la lógica de agendar pedidos. Todos son inmediatos (mayorista y detal). Si cliente pide explícitamente programar un pedido, el bot envía esa info al asesor.**

**Respuesta:** Entendido. Cambio de flujo:

**Actual (antes de tu solicitud):**
- Cliente puede elegir "Entrega Inmediata" o "Entrega Programada"
- Si programa → captura fecha/hora → se persiste

**Nuevo flujo solicitado:**
- Por defecto, todos los pedidos asumen "Entrega Inmediata"
- Si el cliente **explícitamente** dice "Necesito programar" o "Para tal fecha" → el bot lo notifica al asesor con `message_advisor` y el asesor gestiona manualmente
- No hay bucle automático de "¿cuándo lo necesitas?"

**Impacto:**
- Elimina flujo `select_date` / `select_time` / `confirm_schedule`
- Simplifica el prompt del agente
- Asesor se convierte en punto de negociación para fechas especiales
- Código: cambio en `src/ai/agent.rs` y prompt del sistema

**¿Procedemos con este cambio?** Puede ir en el siguiente commit.

---

## 6. Integración de nombre y celular desde WhatsApp

**Confirma: automáticamente se recolecta el nombre y celular desde WhatsApp. ¿Hoy en día se usa algo llamado 'username' en la nueva actualización de WhatsApp? ¿Debemos integrar esa lógica usando docs oficiales? ¿La información persiste o siempre se recolecta automáticamente? ¿Si una persona cambia su nombre qué sucede? ¿Si cambia su dirección?**

**Respuesta:**

### Comportamiento actual:

**Recolección automática:**
- `customer_phone` → capturado automáticamente desde `messages[].from` del webhook Meta
- `customer_name` → capturado automáticamente desde `contacts[].profile.name` si Meta lo incluye en el payload

**Persistencia:**
- Una vez guardados en `conversations.customer_name` y `conversations.customer_phone`, **NO se sobreescriben** con nuevos metadatos del webhook
- Si la BD ya tiene valores, aunque el webhook traiga metadatos nuevos, se conservan los viejos

**Si el cliente cambia su nombre en WhatsApp:**
- El sistema **guarda lo que recibió la primera vez** en el webhook
- Cambios posteriores en el perfil de WhatsApp **NO se reflejan automáticamente** en Trabix
- Para cambiar: el cliente puede editar sus datos en `select_customer_data_field` durante un pedido

**Si el cliente cambia su dirección:**
- No hay persistencia de "dirección preferida anterior"
- La dirección se captura **por cada pedido** en `collect_address`
- Si es el mismo cliente, debe reescribir la dirección

### Username de WhatsApp (Feature nuevo de Meta):

**Estado actual:** ❌ **No integrado** en esta versión del AI agent.

**Qué es:** Meta agregó un campo `username` (handle único del usuario, similar a redes sociales) en el objeto `Contact` del webhook.

**Para integrarlo, necesitarías:**

1. **Leer la documentación oficial de Meta:**
   - https://developers.facebook.com/docs/whatsapp/cloud-api/reference/contacts-object
   - Verificar estructura de `Contact.username` en payloads entrantes

2. **Parsear en `webhook.rs`:**
   ```rust
   // Agregado al webhook payload parsing
   let username = contacts[0].username.clone(); // si existe
   ```

3. **Persistir en DB:**
   - Opción A: Agregar columna `customer_username` en `conversations`
   - Opción B: Guardar solo en `state_data` JSON (si es opcional/transient)

4. **Usar en el agente:**
   - Si captura username en el primer mensaje → se guarda junto con `customer_phone` y `customer_name`
   - ¿Lo usas como ID alternativo? ¿O solo como referencia visual para el asesor?

**¿Quieres que implementemos esto ahora?** Necesito saber:
- ¿Persistimos `username` en columna separada o en JSON?
- ¿Lo mostramos al asesor en los resúmenes de caso?
- ¿Se recolecta automático o es solo si Meta lo trae?

---

## 7. Aceptar solo granizados del menú

**El bot solo acepta pedidos de granizados que efectivamente manejamos, los del menú.**

**Respuesta:** Sí, garantizado. El flujo es a prueba de errores:

**Validación:**
- `get_menu()` obtiene los `flavor_id` válidos (tanto con licor como sin licor)
- Cuando se agrega un item con `add_order_item(flavor_id)`, se valida contra esa lista
- Si el `flavor_id` no existe → tool retorna error
- El cliente no puede completar un pedido con sabores inventados

**Lista oficial de sabores:**

**Con licor (7 sabores):**
- `liquor_maracumango_ron_blanco` → "Maracumango Ron blanco"
- `liquor_blueberry_vodka` → "Blueberry Vodka"
- `liquor_uva_vodka` → "Uva Vodka"
- `liquor_bonbonbum_whiskey` → "Bonbonbum Whiskey"
- `liquor_bonbonbum_fresa_champagne` → "Bonbonbum fresa champaña"
- `liquor_smirnoff_lulo` → "Smirnoff de lulo"
- `liquor_manzana_verde_tequila` → "Manzana verde Tequila"

**Sin licor (4 sabores):**
- `non_liquor_maracumango` → "Maracumango"
- `non_liquor_manzana_verde` → "Manzana verde"
- `non_liquor_bonbonbum` → "Bonbonbum"
- `non_liquor_blueberry` → "Blueberry"

**Ubicación del código:**
- Validación: `src/ai/agent.rs` → `add_order_item()`
- Definición: `config/messages.toml` → `[order.flavors_with_liquor]` y `[order.flavors_without_liquor]`

---

## 8. Cálculo de pedido, domicilio y checkout

**¿El bot realiza cálculos de lo que cuesta el pedido? ¿El domicilio? ¿Entrega el review checkout al cliente para confirmar la información y posteriormente enviarla a logística? ¿El sistema actual permite cambios en la confirmación de pedido?**

**Respuesta:** Sí, completo. Flujo detallado:

### Cálculo y resumen:

**Qué calcula el bot:**
- Subtotal por item (cantidad × precio unitario según tier: detal/20-49/50-99/100+)
- Total estimado (suma de items, **sin domicilio**)
- Domicilio (se agrega después, no aquí)
- Total final (subtotal + domicilio)

**Dónde ocurre:**
- Tool `calculate_order()` → delega a `src/bot/pricing.rs` (determinista, ya probado)
- No es adivinanza del agente; es determinismo matemático

### Review/Resumen para el cliente:

**Estado:** `review_checkout`

**Qué muestra:**
```
🧾 RESUMEN DE TU PEDIDO

Cliente: [nombre]
Teléfono: [teléfono]
Dirección: [dirección]
Entrega: Inmediata

Items:
- 10 x Maracumango (sin licor)
- 5 x Blueberry Vodka (con licor)

Total estimado: $120.000

Nota: el domicilio no está incluido. El asesor lo agregará antes 
de pasar al pago final. Si tu pedido aplica al por mayor y tienes 
código de descuento, podrás usarlo justo antes de elegir el pago.
```

**Botones:**
- `Continuar` → avanza
- `Modificar datos` → permite editar nombre, teléfono o dirección, luego vuelve al resumen

### Cambios de datos:

**Sí, se puede cambiar** en `select_customer_data_field`:
- Elige qué campo: Nombre / Teléfono / Dirección
- Escribe el valor nuevo
- Se valida (nombre 2-80 chars, teléfono solo dígitos 7-15, dirección 5-160)
- Vuelve al resumen con los datos actualizados

### Definición de domicilio:

**Quién lo hace:** El **asesor** (no el cliente)
- Cliente solo dice dónde entrega (dirección)
- Asesor calcula el costo según zona (Armenia norte/centro/sur, o pueblo cercano, o manual)
- Tool: `set_delivery_zone_armenia`, `set_delivery_nearby_town`, `set_manual_delivery_cost`
- Estado: `ask_delivery_cost` (el asesor interactúa aquí)

### Cambios después de confirmar:

**❌ NO hay cambios posteriores** después de que el asesor confirma disponibilidad.

**Por qué:**
- Una vez que `confirm_advisor_availability(available=true)` se ejecuta, el estado cambia a `SelectPaymentMethod` con `total_final` ya definido
- Si el cliente quiere cambiar algo después: debe cancelar el pedido con `cancel_order()` y reiniciar

**Ubicación del código:**
- Cálculo: `src/bot/pricing.rs`
- Resumen: `src/bot/states/checkout.rs` → `render_summary()`
- Agente: `src/ai/agent.rs` → `get_order_summary()`, `finalize_checkout()`, `confirm_advisor_availability()`

---

## 9. Sistema de botones

**¿El sistema sigue permitiendo uso de botones? ¿De ser así, dónde exactamente?**

**Respuesta:** Sí, pero **limitado en el AI agent actual**.

### En el AI agent (Haiku 4.5):

**Tool `show_menu_image`:**
- Envía la imagen del menú
- Genera un `BotAction::SendAssetImage` + botones de respuesta implícitos

**Interacción por texto:**
- El agente NO tiene un tool "crear botón personalizado"
- Responde con texto natural; cliente responde con texto o selecciona de opciones que el bot propone

**Botones que quedan del flujo determinista antiguo:**
- Fallbacks para casos especiales (timeout del asesor, relay, etc.)
- Actions: `BotAction::SendButton`, `BotAction::SendList` (aún disponibles en la capa de transporte)

### En el flujo determinista (no AI agent):

Los estados que NO están bajo el AI agent aún **sí usan botones**: 
- `wait_advisor_response` (botón "Atender" para el asesor)
- `relay_mode` (botón "Finalizar" para cerrar relay)
- Recordatorios de inactividad

### Resumen:

| Componente | Usa botones | Ubicación |
|---|---|---|
| AI Agent Haiku | Limitado (solo imagen) | `src/ai/agent.rs` |
| Flujo determinista | Sí (completo) | `src/bot/states/` |
| Transport layer | Sí (acciones disponibles) | `src/bot/state_machine.rs` |

**¿Quieres que el agente tenga más libertad con botones/listas?** Podríamos agregar tools como `send_buttons()`, `send_list()`.

---

## 10. Timers activos

**¿El bot tiene timers? ¿Cómo de inactividad por parte del cliente, 2 o 5 minutos? ¿Otros timers? ¿Cómo funcionan actualmente y qué hacen?**

**Respuesta:** Sí, sistema consolidado en FASE 5. Catálogo actual:

| Nombre | Duración | Disparador | Qué pasa | Estado(s) |
|--------|----------|-----------|---------|-----------|
| **Comprobante** | 10 min | Cliente debe subir foto de transferencia | Si vence: ofrece "Cambiar pago" o "Cancelar" | `wait_receipt` |
| **Asesor (unificado)** | 5 min | Cualquier espera de asesor (disponibilidad, domicilio, hora) | Si vence: fallback según estado (negociación de hora o `manual_followup`) | `wait_advisor_response`, `ask_delivery_cost`, etc. |
| **Inactividad - Recordatorio** | 2 min | Cliente inactivo en un estado de entrada | Reenvía el prompt actual **una sola vez**; después NO hay reset, el bot queda esperando | Múltiples (no aplica a wait_*) |
| **Relay (solo flujo determinista legado)** | 30 min | Cliente y asesor en conversación directa | Si vence: cierra relay, vuelve a `main_menu` | `relay_mode` |

Los timers eliminados en FASE 5: contacto de asesor (2 min), stuck inmediato (30 min), stuck programado (23 h) y reset por inactividad (35 min).

### Cómo funcionan:

1. **Creación:** Se disparan con `BotAction::StartTimer { timer_type, phone, duration }`
2. **Almacenamiento:** En memoria en-proceso (con respaldo en sweep periódico a BD)
3. **Chequeo:** Sweep cada N segundos verifica `conversations.state_data` para timers vencidos
4. **Vencimiento:** Al vencer, envía una acción final o toma decisión automática
5. **Restauración:** Al reiniciar el bot, `restore_pending_timers()` recrea todos los timers vivos desde la BD

### Estados que NO aplican inactividad:

- `wait_receipt`
- `wait_advisor_response`
- `wait_advisor_contact`
- `relay_mode`
- (Cualquier estado donde hay un timer específico ya activo)

**Ubicación del código:**
- Definición: `src/bot/timers.rs`
- Constantes: `RECEIPT_TIMEOUT`, `ADVISOR_STUCK_TIMEOUT`, etc.
- Sweep: `src/engine.rs` → `sweep_pending_timers()`

---

## 11. Comunicación bot ↔ asesor

**¿Cómo funciona la comunicación entera entre el bot y el asesor? ¿Qué incluye esto? ¿Qué modifica? ¿Qué nuevos escenarios genera esta interacción?**

**Respuesta:** Arquitectura de dos actores con mismo cerebro (bot AI):

### Modelo de comunicación:

```
CLIENTE ← TextMessage → BOT (Claude Haiku 4.5)
ASESOR ← TextMessage → BOT (Claude Haiku 4.5)

Ambos comparten:
- Misma ConversationContext (datos de pedido, cliente, estado)
- Mismo agent_case_messages (historial conversacional)
- Same business logic tools (precios, validaciones, persistencia)
```

### Flujos principales:

#### A. Cliente envía mensaje normal:
1. Bot recibe como `Actor::Customer`
2. Llama al agente con sistema prompt de cliente
3. Agente lee contexto del caso (nombre, teléfono, items, etc.)
4. Si necesita avisar al asesor → **tool `message_advisor`** (envía texto directo)
5. Respuesta normal va al cliente

#### B. Asesor envía mensaje:
1. Bot recibe como `Actor::Advisor` (siempre, sin excepción)
2. Llama al agente con sistema prompt de asesor
3. Agente puede:
   - **Definir domicilio:** `set_delivery_zone_armenia()`, `set_delivery_nearby_town()`, `set_manual_delivery_cost()`
   - **Confirmar disponibilidad:** `confirm_advisor_availability(available=true/false)`
   - **Avisar al cliente:** **tool `message_customer`** (envía texto directo)
4. Respuesta/acción se ejecuta

#### C. Cliente finaliza pedido → Handoff:
1. Cliente dice "Confirmar pedido" o algo similar
2. Bot llama `finalize_checkout()` → estado `AskDeliveryCost`
3. Asesor recibe notificación con resumen completo
4. Asesor interactúa en `ask_delivery_cost` para definir domicilio
5. Asesor confirma con `confirm_advisor_availability(available=true)`
6. Cliente pasa a `select_payment_method`
7. Cliente elige pago
8. Si "Pago Ahora" → espera comprobante
9. Si comprobante llega → confirmación final al asesor

### Datos que se modifican durante interacción:

| Dato | Quién modifica | Cuándo |
|------|---|---|
| `customer_name`, `customer_phone`, `delivery_address` | Bot (de webhook) o cliente (texto) | Captura inicial |
| `items` | Cliente (agregando productos) | Selección de orden |
| `delivery_type`, `scheduled_date`, `scheduled_time` | Cliente (eligiendo entrega) | Después de "Hacer Pedido" |
| `delivery_cost`, `total_final` | Asesor (con tools) | En `ask_delivery_cost` |
| `payment_method`, `receipt_media_id` | Cliente (eligiendo pago) | En `select_payment_method` / `wait_receipt` |
| `referral_code`, `referral_discount_total`, `ambassador_commission_total` | Cliente (ingresando código mayorista) | Antes del pago (si aplica wholesale) |

### Nuevos escenarios con AI agent:

1. **Dato incompleto:** El agente pregunta solo lo faltante (no repite lo que ya sabe)
2. **Múltiples datos en un solo mensaje:** Agente guarda TODO en el mismo turno (ej: "Se llama Juan, vive en Cra 15 #20")
3. **Negociación de hora (fuera del scope v1):** Aún es determinista; el asesor maneja manualmente
4. **Relay directo:** Si cliente pide "hablar con asesor", el flujo aún es determinista (no AI agent)
5. **Consultas fuera de tema:** El agente redirige con amabilidad hacia el pedido

**Ubicación del código:**
- Agents: `src/ai/agent.rs` → `run_customer_turn()`, `run_advisor_turn()`
- Tools de comunicación: `dispatch_tool()` → `"message_customer"`, `"message_advisor"`
- Routing: `src/engine.rs` → decide si es cliente o asesor según `from` del webhook

---

## 12. Sistema de código de referido (mayorista)

**¿El sistema de código de referidos para compras al por mayor cómo funciona actualmente? ¿Se guardan los datos en algún lugar o solo se reenvia al asesor el pedido entero al final (tanto mayorista como detal) incluyendo sus respectivos datos como código referido si es mayorista, o domicilio, o dirección?**

**Respuesta:** Flujo completo con persistencia:

### Cuándo se activa:

**Solo si el pedido tiene wholesale bucket:**
- 20+ unidades de un **mismo tipo** (con licor O sin licor, no mixto)
- Ejemplo: 20+ Maracumango sin licor, O 50+ Blueberry Vodka con licor
- Si es detal puro (menos de 20 unidades) → no se pregunta código

### Flujo paso a paso:

**Estado:** `select_referral_option` (antes de elegir método de pago)

1. **Bot pregunta:** "¿Tienes código de descuento?"
   - Botones: `Tengo código` / `Seguir sin código`

2. **Si `Tengo código`:**
   - Estado → `wait_referral_code`
   - Cliente escribe el código (ej: "trabix-prueba15")
   - Bot normaliza: `trim().to_lowercase()` → "trabix-prueba15"
   - Valida contra `config/referrals.toml`:
     - Solo minúsculas
     - Sin espacios
     - Máximo 15 caracteres
   - Si es válido → aplica descuento; si no → ofrece "Reintentar código" o "Seguir sin código"

3. **Aplicación de descuento:**
   - Solo se aplica a buckets **mayorista**, no a detal
   - Cada bucket recalcula su tier independientemente:

   | Unidades | Descuento cliente | Comisión embajador |
   |---|---|---|
   | 20-49 | 10% | 15% |
   | 50-99 | 12% | 18% |
   | 100+ | 15% | 20% |

   - Si el código está en `boost_codes` → comisión embajador suma +5% puntos (sin cambiar descuento cliente)
   - Descuento cliente **se redondea hacia arriba** al siguiente centenar:
     - Ejemplo: $4.510 → $4.600
     - Si ya es exacto en centenar (ej: $4.500) → se mantiene igual
   - **Domicilio NO participa** en descuento ni comisión

4. **Recálculo de totales:**
   ```
   subtotal_original = suma de todos los items
   referral_discount_total = monto que se quita
   subtotal_con_descuento = subtotal_original - referral_discount_total
   ambassador_commission_total = porcentaje del asesor sobre buckets mayorista
   total_final = subtotal_con_descuento + delivery_cost
   ```

5. **Si `Seguir sin código`:**
   - Se conservan los totales originales sin descuento
   - Pasa directamente a `select_payment_method`

### Qué se persiste:

**En `orders`:**
```sql
referral_code VARCHAR -- ej: "trabix-prueba15"
referral_discount_total INT -- dinero descontado al cliente
ambassador_commission_total INT -- comisión ganada por embajador
```

**En `conversations.state_data` (JSON):**
```json
{
  "referral_code": "trabix-prueba15",
  "referral_has_boost": true,
  "referral_discount_total": 15000,
  "ambassador_commission_total": 45000
}
```

### Qué recibe el asesor:

Cuando el pedido se finaliza, el asesor recibe un resumen que **incluye**:
- Código referido usado
- Descuento aplicado (cliente)
- Comisión ganada (embajador)

### Validación de código:

**Ubicación:** `config/referrals.toml`

```toml
[referral]
codes = [
  "trabix-prueba15",
  "rider332",
  "bytebann"
]

# Opcional: códigos que suman +5% a comisión
boost_codes = [
  "trabix-prueba15"  # Debe existir también en "codes"
]
```

**Ubicación del código:**
- Validación: `src/ai/agent.rs` → `apply_referral_code()`
- Cálculo: `src/bot/pricing.rs` → `calcular_referido_con_boost()`
- Persistencia: `src/db/queries.rs` → `create_order()`, `update_order()`
- Config: `src/referrals.rs` → `referral_registry()`

---

## 13. Métodos de pago: Efectivo vs. Transferencia

**Cuando se llega al final o al pago, los clientes eligen si desean pagar efectivo o por transferencia. ¿El sistema cómo maneja este flujo actualmente?**

**Respuesta:** Dos métodos, dos flujos completamente distintos:

### Estado: `select_payment_method`

Bot ofrece 2 botones:
- `Contra Entrega` (efectivo al recibir)
- `Pago Ahora` (transferencia previa)

### Flujo: Contra Entrega (cash_on_delivery)

```
Cliente presiona "Contra Entrega"
    ↓
set_payment_method(method="cash_on_delivery")
    ↓
Estado persistido → "confirmed"
    ↓
Resumen final enviado al ASESOR:
  "Pedido confirmado (contra entrega):
   Cliente: Juan Pérez
   Productos: ...
   Total: $120.000"
    ↓
Cliente recibe confirmación:
  "¡Pedido confirmado! 🎉"
    ↓
Conversación reinicia → main_menu
    ↓
[FIN - El asesor se contacta con el cliente para coordinar entrega]
```

**Duración:** Instantáneo, sin esperas adicionales

### Flujo: Pago Ahora (transfer/Pago por Transferencia)

```
Cliente presiona "Pago Ahora"
    ↓
set_payment_method(method="transfer")
    ↓
Estado persistido → "waiting_receipt"
    ↓
BOT ENVÍA AUTOMÁTICAMENTE:
  "Instrucciones de transferencia:
   Banco: [X]
   Cuenta: [Y]
   Referencia: [número de pedido]"
   
  [También adjunta los datos de cuenta via SendTransferInstructions action]
    ↓
INICIA TIMER: 10 minutos ⏱️
    ↓
Cliente DEBE enviar IMAGEN (captura del comprobante)
    ↓
  Si llega IMAGEN VÁLIDA:
    ├─ Se guarda media_id del comprobante
    ├─ Se actualiza estado a "confirmed"
    ├─ Resumen + FOTO enviada al ASESOR:
    │   "Pedido confirmado (pago por transferencia):
    │    Cliente: Juan Pérez
    │    Productos: ...
    │    Total: $120.000
    │    [ADJUNTA: captura de comprobante]"
    ├─ Cliente recibe: "¡Comprobante recibido! Tu pedido quedó confirmado 🎉"
    └─ Conversación reinicia → main_menu
    
  Si llega TEXTO u OTRO INPUT:
    ├─ Bot rechaza: "Para validar el pago necesito una foto del comprobante 📸"
    └─ Repite la instrucción (cliente sigue en wait_receipt)
    
  Si TIMER VENCE (10 min):
    ├─ Se marca: receipt_timer_expired = true
    ├─ Bot ofrece botones:
    │   - "Cambiar pago" → vuelve a select_payment_method
    │   - "Cancelar" → cancel_order() → main_menu
    └─ [Espera a que cliente elija]
```

**Duración:** Hasta 10 minutos (o menos si sube comprobante rápido)

### Comparación:

| Aspecto | Contra Entrega | Pago Ahora |
|---------|---|---|
| **Botones cliente** | 1 click → confirma | Upload comprobante |
| **Validación** | Ninguna (asesor verifica after) | Foto requerida |
| **Tiempo** | Instantáneo | Hasta 10 min |
| **Si vence timer** | N/A | Ofrece cambiar o cancelar |
| **Notificación asesor** | Con resumen | Con resumen + foto |

### Atajo para imagen recibida:

**Si el cliente envía una imagen mientras está en `wait_receipt`:**
- Se procesa **100% determinista** (sin llamar al LLM)
- Se ahorra una llamada a Claude Haiku
- Automáticamente se guarda y se notifica al asesor

**Ubicación del código:**
- Métodos: `src/ai/agent.rs` → `set_payment_method()`
- Atajo imagen: `try_handle_receipt_shortcut()`
- Transfer instructions: `src/bot/state_machine.rs` → `BotAction::SendTransferInstructions`
- Config mensajes: `config/messages.toml` → `[checkout]` → `receipt_image_required`

---

## 14. Fricción humana necesaria

**La idea principal del sistema es que se elimine la fricción humana, pero que se requiera cuando algo sucede específicamente (el cliente solicita contacto explícito con otra persona: entregar datos de cliente y mandarlo a asesor para que asesor se contacte con esa persona apenas pueda, o en alguna otra situación). ¿En qué casos específicos es importante la fricción humana requerida?**

**Respuesta:** Catálogo completo de escenarios donde **se REQUIERE** intervención del asesor:

### A. Domicilio fuera del sistema conocido:

**Cuándo:**
- Cliente dice dirección en municipio fuera de Armenia y fuera de la lista de "pueblos cercanos conocidos"

**Flujo:**
1. Bot intenta `lookup_nearby_town(nombre)` → no encuentra
2. Bot usa **`message_advisor`** para contactar asesor:
   ```
   "Cliente solicita entrega en [municipio].
    Necesito que confirmes el valor del domicilio para ese destino.
    (Recuerda: fuera de Armenia el mínimo son 20 unidades)"
   ```
3. Asesor responde con el costo (ej: "$50.000")
4. Bot usa **`set_manual_delivery_cost(50000)`** con el valor confirmado
5. Continúa el flujo normal

**Por qué:** No hay tarifa predefinida; requiere decisión comercial

---

### B. Pedido inmediato y asesor NO puede atender:

**Cuándo:**
- Cliente solicita entrega inmediata
- Asesor confirma que en ese momento no puede despacharlo

**Flujo:**
1. Pedido pasa a `ask_delivery_cost` (asesor ve el caso)
2. Asesor **no responde** o explícitamente dice "No puedo"
3. **Timer de 5 minutos** expira automáticamente
4. Sistema pasa a flujo de negociación de hora (actualmente determinista, no AI agent)
5. O → `manual_followup` (asesor contacta después manualmente)

**Por qué:** No se puede confirmar disponibilidad automáticamente; requiere decisión humana

---

### C. Cliente solicita "Hablar con Asesor":

**Cuándo:**
- Cliente presiona botón `Hablar con Asesor` en cualquier punto
- O cliente dice algo fuera del scope del bot (consultas generales, quejas, etc.)

**Flujo:**
1. Bot pide nombre y teléfono (si no los tiene)
2. Bot muestra resumen: "¿Seguro que quieres hablar con [asesor]?"
3. Si confirma → `wait_advisor_contact` (timer 2 min)
4. Si asesor responde → entra en **relay directo** (cliente ↔ asesor en texto libre)
5. Si timer vence → cliente puede dejar mensaje o volver al menú

**Por qué:** Consulta personal, negociación, o issue que el bot no puede resolver

---

### D. Cliente cambia de método de pago después del timeout:

**Cuándo:**
- Cliente eligió "Pago Ahora"
- Timer de comprobante vence (10 min, sin foto)
- Bot ofrece: "Cambiar pago" o "Cancelar"
- Cliente presiona "Cambiar pago"

**Flujo:**
1. Vuelve a `select_payment_method` (los 2 botones nuevamente)
2. Si elige "Contra Entrega" → confirma igual que siempre
3. Si elige "Pago Ahora" otra vez → reinicia el timer
4. **Asesor debe estar disponible** para cualquier negociación adicional

**Por qué:** Cambio de circunstancias; requiere confirmar nuevamente

---

### E. Pedido en estado `manual_followup`:

**Cuándo:**
- Asesor dijo explícitamente "No puedo" → `confirm_advisor_availability(available=false)`
- O timer de 30 minutos en `ask_delivery_cost` expiró
- O negociación de hora falló

**Flujo:**
- Orden queda en estado `manual_followup`
- El asesor recibe notificación implícita de que debe contactar al cliente directamente (fuera del bot)
- Contacto: WhatsApp personal, teléfono, o reunión presencial

**Por qué:** No hay ruta automática; requiere gestión comercial

---

### F. Consulta fuera de tema:

**Cuándo:**
- Cliente pregunta algo no relacionado con pedido (horarios de tienda, cambios, devoluciones, etc.)

**Flujo:**
1. Agente Haiku intenta redirigir: "Para eso mejor habla con un asesor"
2. Si cliente insiste → se sugiere "Hablar con Asesor"
3. Entra en relay directo

**Por qué:** El bot no tiene información de otros temas

---

### G. Dato que no se puede validar automáticamente:

**Cuándo:**
- Cliente escribe una dirección ambigua (ej: "Cra 15" sin número completo)
- Teléfono tiene formato extraño pero no es completamente inválido
- Nombre muy corto (1 carácter) pero cliente insiste

**Flujo:**
1. Bot valida con reglas estrictas
2. Si falla → pide repetir
3. Si cliente insiste en algo que no cumple → asesor puede hacer override manual via `set_customer_field()` tool

**Por qué:** Las reglas de validación son deterministas; excepciones requieren criterio humano

---

### Casos donde **NO** se requiere fricción (bot resuelve solo):

| Escenario | Por qué |
|-----------|---------|
| Datos incompletos (nombre, teléfono, dirección) | Bot pregunta qué falta; cliente responde |
| Sabor no válido | Bot valida contra menú; rechaza o pide aclaración |
| Cantidad fuera de rango | Bot valida (1-999 unidades); rechaza e intenta nuevamente |
| Código de referido inválido | Bot valida contra `config/referrals.toml`; ofrece reintentar |
| Domicilio en Armenia | Bot pregunta zona (norte/centro/sur) y aplica costo automático |
| Domicilio en pueblo cercano conocido | Bot busca en lista y aplica costo automático |
| Precios y cálculos | Bot usa `pricing.rs` determinista |
| Timer de comprobante (10 min) | Sistema automático; no requiere intervención |
| Timer de inactividad cliente | Sistema automático; reinicia conversación |

---

## Resumen de arquitectura

**Objetivo principal:** Eliminar fricción innecesaria, mantener fricción donde agrega valor comercial.

**Bot:**
- Recopila datos (nombre, teléfono, dirección, productos)
- Valida contra reglas deterministas (cantidad, sabor, formato)
- Calcula precios y cálculos
- Administra timers y estado conversacional
- Redirige al asesor cuando:
  - La situación requiere criterio comercial (ej: tarifa fuera de catálogo)
  - El cliente lo pide explícitamente
  - Una regla automática no tiene respuesta clara

**Asesor:**
- Define domicilio cuando no está en catálogo
- Confirma disponibilidad para entrega inmediata
- Maneja negociaciones de hora (futuro: bajo AI agent también)
- Resuelve consultas personalizadas
- Contacta manualmente si no hay respuesta automática

---

**Última actualización:** 2026-07-13  
**Versión del bot:** AI Agent v1.0 (Claude Haiku 4.5)
