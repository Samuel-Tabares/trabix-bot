# Bot WhatsApp Granizados — Especificación Técnica de Implementación

> Este documento es la fuente de verdad para implementar el bot. Contiene arquitectura, estados, reglas de negocio, modelo de datos, estructura de proyecto y decisiones técnicas. El flujo visual completo está en `Flow_Design_Diagram.mermaid`.

---

## 1. Visión general del sistema

Bot conversacional para WhatsApp que gestiona pedidos de granizados. Funciona como una **máquina de estados** donde el cliente interactúa mediante botones interactivos de WhatsApp. Un asesor humano interviene cuando es necesario, también mediante botones, y el bot actúa como intermediario en toda comunicación (el asesor nunca escribe directamente al cliente).

### Decisiones de arquitectura

- **Patrón**: Webhook — Meta envía cada mensaje como HTTP POST al servidor. El servidor procesa, consulta estado en DB, ejecuta transición, responde vía API de Meta.
- **El bot nunca se "apaga"**: No existe concepto de desactivación. El bot siempre es el intermediario. Para pedidos al mayor y "hablar con asesor", entra en **Modo Relay** (reenvía mensajes entre asesor y cliente).
- **El asesor interactúa con el bot vía botones** enviados a su número personal de WhatsApp. El asesor NO tiene acceso directo al número del negocio (la Cloud API de Meta desvincula la app WhatsApp Business normal).
- **Timers en memoria con persistencia**: Los temporizadores corren como tareas async de Tokio. Si el servidor se reinicia, se recrean consultando `last_message_at` de la DB.

---

## 2. Stack tecnológico

| Crate | Versión | Propósito |
|-------|---------|-----------|
| `axum` | 0.7+ | Framework web async. Rutas, extractors, middleware. |
| `tokio` | 1.x (features: full) | Runtime async. Event loop, timers, spawn de tareas. |
| `reqwest` | 0.12+ | Cliente HTTP async para llamar API de Meta. |
| `serde` / `serde_json` | 1.x | Serialización de payloads de WhatsApp. |
| `sqlx` | 0.8+ (feature: postgres, runtime-tokio) | Driver PostgreSQL async. Queries verificadas en compile time. |
| `hmac` / `sha2` | latest | Validación de firma HMAC-SHA256 de webhooks de Meta. |
| `chrono` | 0.4+ | Fechas, horas, timestamps, horarios de atención. |
| `tracing` / `tracing-subscriber` | 0.1+ | Logging estructurado (visible en dashboard de Railway). |
| `dotenvy` | latest | Variables de entorno desde `.env` en desarrollo. |
| `tower-http` | 0.5+ | Middleware CORS y logging para Axum. |
| `tokio-util` | latest | `CancellationToken` para cancelar timers anticipadamente. |

---

## 3. Variables de entorno

```env
WHATSAPP_TOKEN=<token_de_acceso_permanente_system_user>
WHATSAPP_PHONE_ID=<phone_number_id_del_numero_de_negocio>
WHATSAPP_VERIFY_TOKEN=<string_secreto_definido_por_ti_para_verificacion_webhook>
WHATSAPP_APP_SECRET=<app_secret_para_validar_firmas_hmac>
DATABASE_URL=postgresql://user:pass@host:5432/granizado_bot
ADVISOR_PHONE=<numero_whatsapp_personal_del_asesor_con_codigo_pais>
PORT=8080
```

---

## 4. Estructura de proyecto

```
granizado-bot/
├── src/
│   ├── main.rs                  # Entry point: configura Axum, pool DB, lanza servidor
│   ├── config.rs                # Carga y valida variables de entorno
│   ├── routes/
│   │   ├── mod.rs               # Registro de rutas en Router de Axum
│   │   ├── webhook.rs           # POST /webhook → recibe mensajes de Meta
│   │   └── verify.rs            # GET /webhook → verificación inicial de Meta
│   ├── whatsapp/
│   │   ├── mod.rs
│   │   ├── client.rs            # send_text(), send_buttons(), send_list(), send_image(), mark_as_read()
│   │   ├── types.rs             # Structs Serde para payloads entrantes y salientes de WhatsApp
│   │   ├── buttons.rs           # Builders de mensajes interactivos (botones, listas)
│   │   └── media.rs             # Envío de imágenes (menú de sabores)
│   ├── bot/
│   │   ├── mod.rs
│   │   ├── state_machine.rs     # Enum ConversationState + fn transition(state, input) -> (new_state, action)
│   │   ├── states/
│   │   │   ├── menu.rs          # MainMenu, ViewMenu, ViewSchedule
│   │   │   ├── scheduling.rs    # WhenDelivery, CheckSchedule, OutOfHours, SelectDate, SelectTime, ConfirmSchedule
│   │   │   ├── data_collect.rs  # CollectName, CollectPhone, CollectAddress
│   │   │   ├── order.rs         # SelectType, SelectFlavor, SelectQuantity, AddMore
│   │   │   ├── checkout.rs      # ShowSummary, WaitReceipt, ReceiptOk, ReceiptTimeout
│   │   │   ├── advisor.rs       # NotifyAdvisor, WaitAdvisor, AskDeliveryCost, NegotiateHour, ConfirmOrder
│   │   │   └── relay.rs         # RelayInit, RelayMode, RelayEnd
│   │   ├── pricing.rs           # Cálculo de precios, promos, clasificación detal/mayor
│   │   └── timers.rs            # Gestión de temporizadores con CancellationToken
│   └── db/
│       ├── mod.rs               # Pool de conexiones PgPool
│       ├── models.rs            # Structs: Conversation, Order, OrderItem
│       └── queries.rs           # CRUD: get_conversation(), upsert_state(), create_order(), etc.
├── migrations/                  # Archivos SQL para sqlx-cli
│   ├── 001_create_conversations.sql
│   ├── 002_create_orders.sql
│   └── 003_create_order_items.sql
├── Cargo.toml
├── Dockerfile                   # Multi-stage build
└── .env.example
```

---

## 5. Máquina de estados

### 5.1 Enum de estados

```rust
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "conversation_state", rename_all = "snake_case")]
pub enum ConversationState {
    // Menú y navegación
    MainMenu,
    ViewMenu,
    ViewSchedule,

    // Tiempo de entrega
    WhenDelivery,
    CheckSchedule,
    OutOfHours,
    SelectDate,
    SelectTime,
    ConfirmSchedule,

    // Recolección de datos
    CollectName,
    CollectPhone,
    CollectAddress,

    // Selección de granizados
    SelectType,
    SelectFlavor { has_liquor: bool },
    SelectQuantity { has_liquor: bool, flavor: String },
    AddMore,

    // Confirmación de dirección, resumen y pago
    ConfirmAddress,            // Cliente confirma dirección DESPUÉS de elegir forma de pago (ambos paths convergen aquí)
    ShowSummary,               // Calcula precios y muestra resumen con opciones de pago
    WaitReceipt,

    // Interacción con asesor (detal)
    WaitAdvisorResponse,
    AskDeliveryCost,
    NegotiateHour,
    OfferHourToClient { proposed_hour: String },
    WaitClientHour,
    WaitAdvisorHourDecision { client_hour: String },
    WaitAdvisorConfirmHour,    // Asesor confirma pedido después de acordar hora con cliente

    // Pedidos al mayor y hablar con asesor
    WaitAdvisorMayor,
    RelayMode,                 // RELAY_INIT no es un estado separado: es la acción de entrada al transicionar a RelayMode

    // Contactar asesor (menú opción 4)
    ContactAdvisorName,
    ContactAdvisorPhone,
    WaitAdvisorContact,
    LeaveMessage,

    // Fin — estado TRANSITORIO: resetea la conversación a MainMenu inmediatamente.
    // No se persiste. Al ejecutar OrderComplete se limpia state_data y se pone MainMenu.
    OrderComplete,
}
```

### 5.2 Input del usuario

```rust
#[derive(Debug)]
pub enum UserInput {
    ButtonPress(String),       // ID del botón presionado
    TextMessage(String),       // Texto libre
    ImageMessage(String),      // media_id de imagen (comprobante)
    ListSelection(String),     // Selección de lista interactiva
}
```

### 5.3 Acciones de salida

```rust
#[derive(Debug)]
pub enum BotAction {
    SendText { to: String, body: String },
    SendButtons { to: String, body: String, buttons: Vec<Button> },
    SendList { to: String, body: String, sections: Vec<ListSection> },
    SendImage { to: String, media_id: String, caption: Option<String> },
    StartTimer { timer_type: TimerType, phone: String, duration: Duration },
    CancelTimer { timer_type: TimerType, phone: String },
    SaveOrder { order: Order },
    ResetConversation { phone: String },
    RelayMessage { from: String, to: String, body: String },
    NoOp,
}
```

### 5.4 Función de transición (pseudocódigo)

La función de transición es **pura**: no hace I/O. Solo recibe estado + input + contexto y retorna nuevo estado + acciones a ejecutar. El executor en `webhook.rs` se encarga de ejecutar las acciones (enviar mensajes, iniciar timers, guardar en DB).

```rust
fn transition(
    state: &ConversationState,
    input: &UserInput,
    context: &mut ConversationContext,
) -> Result<(ConversationState, Vec<BotAction>)> {
    match (state, input) {
        (MainMenu, ButtonPress(id)) => match id.as_str() {
            "make_order" => (WhenDelivery, vec![SendButtons { ... }]),
            "view_menu" => (ViewMenu, vec![SendImage { ... }, SendButtons { ... }]),
            "view_schedule" => (ViewSchedule, vec![SendText { ... }, SendButtons { ... }]),
            "contact_advisor" => (ContactAdvisorName, vec![SendText { body: "¿Nombre completo?" }]),
            _ => (MainMenu, vec![SendText { body: "Opción no válida" }]),
        },
        // ... demás transiciones
    }
}
```

---

## 6. Reglas de negocio — Precios

### 6.1 Granizados CON LICOR (detal: < 20 unidades)

La promoción aplica en **pares**: por cada 2 granizados, el segundo cuesta $4.000.

```rust
fn calcular_precio_licor_detal(cantidad: u32) -> u32 {
    let pares = cantidad / 2;
    let impares = cantidad % 2;
    (pares * 12_000) + (impares * 8_000)
}
```

| Cantidad | Cálculo | Total |
|----------|---------|-------|
| 1 | 0 pares × $12.000 + 1 × $8.000 | $8.000 |
| 2 | 1 par × $12.000 | $12.000 |
| 3 | 1 par × $12.000 + 1 × $8.000 | $20.000 |
| 4 | 2 pares × $12.000 | $24.000 |
| 5 | 2 pares × $12.000 + 1 × $8.000 | $32.000 |

### 6.2 Granizados SIN LICOR (detal: < 20 unidades)

Precio fijo: **$7.000 por unidad**. Sin promoción.

```rust
fn calcular_precio_sin_licor_detal(cantidad: u32) -> u32 {
    cantidad * 7_000
}
```

### 6.3 Precios al mayor (≥ 20 unidades del MISMO tipo)

La clasificación se evalúa **por separado** para con licor y sin licor. Un pedido puede tener parte detal y parte mayor simultáneamente. Cuando aplica precio de mayor, la promo de pares NO aplica.

```rust
fn precio_unitario_mayor(cantidad: u32, has_liquor: bool) -> u32 {
    match (has_liquor, cantidad) {
        (true, 20..=49)   => 4_900,
        (true, 50..=99)   => 4_700,
        (true, 100..)     => 4_500,
        (false, 20..=49)  => 4_800,
        (false, 50..=99)  => 4_500,
        (false, 100..)    => 4_200,
        _ => unreachable!("Solo se llama con cantidad >= 20"),
    }
}
```

### 6.4 Domicilio

Rango: **$3.500 — $8.000** dentro de Armenia, Quindío. El valor exacto lo define el asesor después de confirmar el pedido (paso `AskDeliveryCost`). El asesor digita el valor como texto (ej: "5000") y el bot lo suma al total.

### 6.5 Horario de atención

Entrega inmediata disponible de **8:00 AM a 11:00 PM** (hora Colombia, UTC-5). Fuera de ese rango el bot ofrece: programar para después, contactar asesor, o volver al menú. Se valida con `chrono` usando timezone `America/Bogota`.

---

## 7. Modelo de datos (PostgreSQL)

### 7.1 conversations

```sql
CREATE TABLE conversations (
    id              SERIAL PRIMARY KEY,
    phone_number    VARCHAR(20) UNIQUE NOT NULL,  -- Número WhatsApp del cliente
    state           VARCHAR(50) NOT NULL DEFAULT 'main_menu',
    state_data      JSONB DEFAULT '{}',           -- Datos temporales del estado actual
    customer_name   VARCHAR(100),
    customer_phone  VARCHAR(20),
    delivery_address TEXT,
    last_message_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

`state_data` (JSONB) almacena datos acumulados durante el flujo:
```json
{
  "items": [
    { "flavor": "maracuyá", "has_liquor": true, "quantity": 2 },
    { "flavor": "mora", "has_liquor": false, "quantity": 3 }
  ],
  "delivery_type": "immediate",
  "scheduled_date": null,
  "scheduled_time": null,
  "payment_method": null,
  "receipt_media_id": null
}
```

### 7.2 orders

```sql
CREATE TABLE orders (
    id              SERIAL PRIMARY KEY,
    conversation_id INT NOT NULL REFERENCES conversations(id),
    delivery_type   VARCHAR(20) NOT NULL,        -- 'immediate' | 'scheduled'
    scheduled_date  DATE,
    scheduled_time  TIME,
    payment_method  VARCHAR(20) NOT NULL,         -- 'cash_on_delivery' | 'transfer'
    receipt_media_id VARCHAR(100),                -- Media ID del comprobante (null si pago contra entrega)
    delivery_cost   INT,                          -- Costo domicilio confirmado por asesor (null hasta confirmación)
    total_estimated INT NOT NULL,                 -- Total sin domicilio
    total_final     INT,                          -- Total con domicilio (null hasta confirmación)
    status          VARCHAR(20) NOT NULL DEFAULT 'pending', -- 'pending' | 'confirmed' | 'cancelled'
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

### 7.3 order_items

```sql
CREATE TABLE order_items (
    id          SERIAL PRIMARY KEY,
    order_id    INT NOT NULL REFERENCES orders(id),
    flavor      VARCHAR(50) NOT NULL,
    has_liquor  BOOLEAN NOT NULL,
    quantity    INT NOT NULL,
    unit_price  INT NOT NULL,                    -- Precio unitario aplicado
    subtotal    INT NOT NULL,                    -- Para con licor detal, ya incluye la promo de pares
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

## 8. Sistema de temporizadores

### 8.1 Tipos de timer

```rust
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum TimerType {
    AdvisorResponse,   // 2 minutos — esperando que el asesor responda
    ReceiptUpload,     // 10 minutos — esperando comprobante del cliente
    RelayInactivity,   // 30 minutos — inactividad en modo relay
    ConversationAbandon, // 15 minutos — cliente dejó de responder durante recolección de datos (mejora futura)
}
```

### 8.2 Implementación

```rust
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type TimerKey = (String, TimerType); // (phone_number, timer_type)
pub type TimerMap = Arc<Mutex<HashMap<TimerKey, CancellationToken>>>;

pub async fn start_timer(
    timers: TimerMap,
    key: TimerKey,
    duration: Duration,
    on_expire: impl FnOnce() + Send + 'static,
) {
    let token = CancellationToken::new();
    let cloned_token = token.clone();

    // Cancelar timer previo si existe
    {
        let mut map = timers.lock().await;
        if let Some(old) = map.insert(key.clone(), token) {
            old.cancel();
        }
    }

    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(duration) => {
                on_expire();
            }
            _ = cloned_token.cancelled() => {
                // Timer cancelado, no hacer nada
            }
        }
    });
}
```

### 8.3 Recuperación tras reinicio

Al arrancar el servidor, consultar conversaciones con estados que impliquen timers activos:
```sql
SELECT phone_number, state, last_message_at
FROM conversations
WHERE state IN ('wait_advisor_response', 'wait_receipt', 'relay_mode', 'wait_advisor_mayor', 'wait_advisor_contact');
```
Para cada una, calcular `remaining = duration - (now - last_message_at)`. Si `remaining > 0`, crear el timer con esa duración. Si `remaining <= 0`, ejecutar la acción de timeout inmediatamente.

---

## 9. Webhooks de WhatsApp

### 9.1 Verificación (GET /webhook)

Meta envía un GET con query params para verificar el webhook al configurarlo:
```
GET /webhook?hub.mode=subscribe&hub.verify_token=TU_VERIFY_TOKEN&hub.challenge=CHALLENGE
```
Responder con `hub.challenge` como texto plano si `hub.verify_token` coincide. Status 200.

### 9.2 Recepción de mensajes (POST /webhook)

Validar firma HMAC-SHA256 del header `X-Hub-Signature-256` contra el body usando `WHATSAPP_APP_SECRET`. Si no coincide, retornar 401.

Siempre retornar **200 OK inmediatamente** (antes de procesar). Meta reintenta si no recibe 200 en 5 segundos.

#### Payload de mensaje de texto entrante (estructura relevante):
```json
{
  "entry": [{
    "changes": [{
      "value": {
        "messages": [{
          "from": "573001234567",
          "type": "text",
          "text": { "body": "Hola" },
          "id": "wamid.xxx"
        }]
      }
    }]
  }]
}
```

#### Payload de botón interactivo presionado:
```json
{
  "entry": [{
    "changes": [{
      "value": {
        "messages": [{
          "from": "573001234567",
          "type": "interactive",
          "interactive": {
            "type": "button_reply",
            "button_reply": {
              "id": "make_order",
              "title": "Hacer Pedido"
            }
          }
        }]
      }
    }]
  }]
}
```

#### Payload de imagen (comprobante):
```json
{
  "entry": [{
    "changes": [{
      "value": {
        "messages": [{
          "from": "573001234567",
          "type": "image",
          "image": {
            "id": "media_id_xxx",
            "mime_type": "image/jpeg"
          }
        }]
      }
    }]
  }]
}
```

### 9.3 Envío de mensajes

**Base URL**: `https://graph.facebook.com/v21.0/{WHATSAPP_PHONE_ID}/messages`

**Header**: `Authorization: Bearer {WHATSAPP_TOKEN}`

#### Enviar texto:
```json
{
  "messaging_product": "whatsapp",
  "to": "573001234567",
  "type": "text",
  "text": { "body": "Hola, bienvenido" }
}
```

#### Enviar botones interactivos (máximo 3 botones):
```json
{
  "messaging_product": "whatsapp",
  "to": "573001234567",
  "type": "interactive",
  "interactive": {
    "type": "button",
    "body": { "text": "¿Qué deseas hacer?" },
    "action": {
      "buttons": [
        { "type": "reply", "reply": { "id": "make_order", "title": "🧊 Hacer Pedido" } },
        { "type": "reply", "reply": { "id": "view_menu", "title": "📋 Ver Menú" } },
        { "type": "reply", "reply": { "id": "view_schedule", "title": "⏰ Horarios" } }
      ]
    }
  }
}
```

> **Limitación importante**: WhatsApp permite máximo **3 botones** por mensaje. Si necesitas más opciones (como el menú principal con 4), usa una **lista interactiva** (ver abajo) o divide en 2 mensajes.

#### Enviar lista interactiva (hasta 10 opciones):
```json
{
  "messaging_product": "whatsapp",
  "to": "573001234567",
  "type": "interactive",
  "interactive": {
    "type": "list",
    "body": { "text": "¿Qué deseas hacer?" },
    "action": {
      "button": "Ver opciones",
      "sections": [{
        "title": "Menú Principal",
        "rows": [
          { "id": "make_order", "title": "🧊 Hacer Pedido", "description": "Arma tu pedido de granizados" },
          { "id": "view_menu", "title": "📋 Ver Menú", "description": "Sabores y precios" },
          { "id": "view_schedule", "title": "⏰ Horarios", "description": "Horarios de entrega" },
          { "id": "contact_advisor", "title": "👨‍💼 Hablar con Asesor", "description": "Contactar a un asesor" }
        ]
      }]
    }
  }
}
```

#### Enviar imagen:
```json
{
  "messaging_product": "whatsapp",
  "to": "573001234567",
  "type": "image",
  "image": {
    "id": "media_id_previamente_subido",
    "caption": "Menú de sabores con licor"
  }
}
```

#### Marcar mensaje como leído:
```json
{
  "messaging_product": "whatsapp",
  "status": "read",
  "message_id": "wamid.xxx"
}
```

---

## 10. Modo Relay

El Modo Relay se activa en dos escenarios: pedidos al mayor (≥20 unidades de un tipo) y cuando el cliente elige "Hablar con Asesor" y el asesor presiona "Atender".

### 10.1 Comportamiento

1. **Activación** (esto es la acción de entrada, NO un estado separado): El bot envía un mensaje al cliente ("Un asesor lo atenderá") y al asesor ("Modo relay activo. Todo lo que escriba se reenviará. Presione 🔴 Finalizar cuando termine."). Luego transiciona directamente a `RelayMode`.
2. **Durante el relay**: Cada mensaje de texto del cliente se reenvía al asesor con prefijo `[CLIENTE ...4567]:`. Cada mensaje del asesor (excepto el botón Finalizar) se reenvía al cliente sin prefijo (parece que habla el negocio directamente).
3. **Finalización**: El asesor presiona el botón "Finalizar" o pasan 30 minutos sin mensajes de nadie. El bot envía mensaje de cierre al cliente y transiciona a `OrderComplete` (que resetea a `MainMenu`).

### 10.2 Identificación de mensajes del asesor

Cuando llega un mensaje al webhook, se compara `from` con `ADVISOR_PHONE`:
- Si `from == ADVISOR_PHONE`: es el asesor. Buscar si hay alguna conversación en estado relay o esperando respuesta de asesor, y procesarlo.
- Si `from != ADVISOR_PHONE`: es un cliente. Buscar su conversación por `phone_number`.

### 10.3 Regla de los 4 últimos dígitos — desambiguación de múltiples clientes

**Problema**: Cuando el asesor tiene 2+ clientes pendientes y el bot le pide texto libre (valor del domicilio, hora propuesta), ¿cómo sabe el bot a qué cliente se refiere?

**Solución**: CADA mensaje que el bot envía al asesor incluye los últimos 4 dígitos del teléfono del cliente como identificador visual:
- "Pedido [...4567] confirmado. ¿Cuánto cobra de domicilio para Cra 15 #20?"
- "Cliente [...4567] propone las 3pm. ¿Puede?"

**Para botones**: El `button_id` incluye el phone completo: `confirm_573001234567`, `cannot_573001234567`. Esto permite al bot identificar automáticamente a qué cliente se refiere.

**Para texto libre del asesor**: El bot identifica al cliente destinatario buscando cuál conversación está en un estado que espera texto del asesor (`AskDeliveryCost`, `NegotiateHour`). Si hay más de una en ese estado simultáneamente (raro pero posible), el bot usa la más reciente (por `last_message_at`). En caso de ambigüedad real, el bot responde al asesor: "Tiene 2 pedidos pendientes. ¿Para cuál cliente? [...4567] o [...8901]?" con botones.

---

## 10.4 Límite de 3 botones de WhatsApp

WhatsApp permite máximo **3 botones interactivos** por mensaje. Cuando se necesitan 4+ opciones, usar **lista interactiva** en vez de botones.

Los pasos del flujo que tienen 2-3 opciones pueden usar botones normales.

---

## 11. Dockerfile (multi-stage build)

```dockerfile
# Stage 1: Build
FROM rust:1.77-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY migrations/ migrations/
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/granizado-bot /usr/local/bin/
CMD ["granizado-bot"]
```

Imagen final: ~30-50 MB. Railway detecta el Dockerfile y lo construye automáticamente en cada push a GitHub.

---

## 12. Flujo de procesamiento de un mensaje (secuencia completa)

```
1. Meta envía POST /webhook con payload JSON
2. Axum handler:
   a. Validar firma HMAC-SHA256
   b. Responder 200 OK inmediatamente
   c. Parsear payload → extraer (phone, input_type, content)
3. Determinar si es mensaje del asesor o del cliente
4. Consultar DB: SELECT state, state_data FROM conversations WHERE phone_number = $1
   - Si no existe → crear con state = 'main_menu'
5. Llamar transition(state, input, context) → (new_state, actions)
6. Ejecutar cada BotAction:
   - SendText/SendButtons/SendImage → POST a API de Meta
   - StartTimer → tokio::spawn con CancellationToken
   - CancelTimer → buscar en TimerMap y cancelar
   - SaveOrder → INSERT en orders + order_items
   - RelayMessage → reenviar al destinatario vía API de Meta
   - ResetConversation → UPDATE state = 'main_menu', limpiar state_data
7. UPDATE conversations SET state = $new_state, state_data = $data, last_message_at = NOW()
```