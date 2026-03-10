# GUÍA DE IMPLEMENTACIÓN — Bot WhatsApp Granizados

> **INSTRUCCIONES PARA EL ASISTENTE DE PROGRAMACIÓN**
>
> Este documento define EXACTAMENTE qué hacer en cada fase para construir el bot. Debes seguir cada paso en orden, sin saltarte ninguno, sin agregar funcionalidad que no esté especificada, y sin avanzar a la siguiente fase hasta completar y validar la actual.
>
> **Reglas generales:**
> - NO agregues funcionalidad, crates, estados, o endpoints que no estén en la spec.
> - NO cambies nombres de archivos, structs, enums, o tablas definidos en la spec.
> - NO uses `unwrap()` en código de producción. Usa `?` con manejo de errores apropiado.
> - NO hagas commits parciales: cada fase debe compilar y funcionar al completarla.
> - SI encuentras una ambigüedad en la spec, pregunta antes de asumir.
> - Usa `tracing::info!` y `tracing::error!` para logging en cada paso importante.
> - Cada fase termina con una sección de VALIDACIÓN. No avances hasta que todos los checks pasen.

---

## FASE 1: Infraestructura base

**Objetivo**: Servidor Axum corriendo en Railway que recibe webhooks de WhatsApp, valida firmas, parsea mensajes, y puede responder con texto y botones.

---

### Paso 1.1 — Inicializar proyecto

**Qué hacer:**
1. Crear proyecto con `cargo init granizado-bot`.
2. Configurar `Cargo.toml` con las dependencias exactas listadas en la sección 2 de `Software_Design_Document.md`.
3. Crear el archivo `.env.example` con las variables de la sección 3 de la spec.
4. Crear `.gitignore` con: `target/`, `.env`, `*.pdb`.

**Archivos a crear:**
- `Cargo.toml`
- `.env.example`
- `.gitignore`

**Criterio de aceptación:**
- `cargo check` compila sin errores.

**NO hacer:**
- No agregar crates que no estén en la spec.
- No crear estructura de carpetas aún (se hace en los siguientes pasos).

---

### Paso 1.2 — Módulo de configuración

**Qué hacer:**
1. Crear `src/config.rs`.
2. Definir un struct `Config` que cargue TODAS las variables de entorno de la sección 3 de la spec.
3. Usar `dotenvy` para cargar `.env` en desarrollo.
4. Implementar un método `Config::from_env()` que lea las variables y retorne `Result<Config, ConfigError>`.
5. Cada variable faltante debe dar un error descriptivo (no un panic genérico).

**Struct esperado:**
```rust
pub struct Config {
    pub whatsapp_token: String,
    pub whatsapp_phone_id: String,
    pub whatsapp_verify_token: String,
    pub whatsapp_app_secret: String,
    pub database_url: String,
    pub advisor_phone: String,
    pub port: u16,
}
```

**Archivos a crear:**
- `src/config.rs`

**Criterio de aceptación:**
- Con un `.env` completo, `Config::from_env()` retorna Ok.
- Con una variable faltante, retorna un error que indica cuál falta.

**NO hacer:**
- No hardcodear valores por defecto para tokens o secretos.
- El único valor con default es `port` (8080).

---

### Paso 1.3 — Servidor Axum y rutas base

**Qué hacer:**
1. Crear `src/main.rs` que:
   - Cargue la configuración.
   - Inicialice `tracing_subscriber` para logging.
   - Cree el `Router` de Axum con dos rutas: `GET /webhook` y `POST /webhook`.
   - Arranque el servidor en `0.0.0.0:{PORT}`.
2. Crear `src/routes/mod.rs`, `src/routes/verify.rs`, `src/routes/webhook.rs`.
3. `GET /webhook` (verify.rs): Implementar la verificación de Meta según sección 9.1 de la spec. Recibe query params `hub.mode`, `hub.verify_token`, `hub.challenge`. Si el token coincide con `WHATSAPP_VERIFY_TOKEN`, responde con `hub.challenge` como texto plano con status 200. Si no coincide, responde 403.
4. `POST /webhook` (webhook.rs): Por ahora solo loguear el body recibido y responder 200 OK. La validación HMAC y el parseo se hacen en los siguientes pasos.

**Archivos a crear:**
- `src/main.rs`
- `src/routes/mod.rs`
- `src/routes/verify.rs`
- `src/routes/webhook.rs`

**Criterio de aceptación:**
- `cargo run` inicia el servidor sin errores.
- `curl "localhost:8080/webhook?hub.mode=subscribe&hub.verify_token=TEST&hub.challenge=abc123"` con el verify_token correcto responde `abc123`.
- `curl -X POST localhost:8080/webhook -d '{}'` responde 200.

**NO hacer:**
- No agregar middleware de CORS todavía.
- No conectar la base de datos todavía.
- No procesar el body del POST todavía.

---

### Paso 1.4 — Validación HMAC-SHA256

**Qué hacer:**
1. En `webhook.rs`, antes de procesar el body del POST, validar la firma del header `X-Hub-Signature-256`.
2. El header tiene formato: `sha256=HASH_HEX`.
3. Calcular HMAC-SHA256 del body raw usando `WHATSAPP_APP_SECRET` como clave.
4. Comparar el hash calculado con el del header.
5. Si no coincide o el header no existe, responder 401 Unauthorized.
6. Si coincide, continuar con el procesamiento.

**Importante:**
- Necesitas acceso al body raw (bytes) para calcular el HMAC y luego parsearlo como JSON. Usar `axum::body::Bytes` como extractor.

**Criterio de aceptación:**
- Un POST con firma válida responde 200.
- Un POST sin header `X-Hub-Signature-256` responde 401.
- Un POST con firma incorrecta responde 401.

**NO hacer:**
- No desactivar la validación "por ahora" para facilitar pruebas. Implementarla correctamente desde el inicio.

---

### Paso 1.5 — Tipos de WhatsApp (Serde structs)

**Qué hacer:**
1. Crear `src/whatsapp/mod.rs`, `src/whatsapp/types.rs`.
2. Definir structs Serde para TODOS los payloads de la sección 9 de la spec.
3. Los structs deben manejar los tipos de mensajes que el bot va a recibir: texto, botón interactivo presionado (button_reply), selección de lista (list_reply), e imagen.

**Structs de entrada (deserialización de webhooks):**
```
WebhookPayload
  └── Entry
        └── Change
              └── Value
                    ├── messages: Vec<IncomingMessage>
                    └── contacts: Vec<Contact>

IncomingMessage
  ├── from: String (número del remitente)
  ├── id: String (wamid)
  ├── message_type: String ("text", "interactive", "image")
  ├── text: Option<TextContent>
  ├── interactive: Option<InteractiveContent>
  └── image: Option<ImageContent>

InteractiveContent
  ├── interactive_type: String ("button_reply", "list_reply")
  ├── button_reply: Option<ButtonReply> { id, title }
  └── list_reply: Option<ListReply> { id, title, description }
```

**Structs de salida (serialización para enviar mensajes):**
```
OutgoingTextMessage { messaging_product, to, type, text: { body } }
OutgoingButtonMessage { messaging_product, to, type, interactive: { type, body, action: { buttons } } }
OutgoingListMessage { messaging_product, to, type, interactive: { type, body, action: { button, sections } } }
OutgoingImageMessage { messaging_product, to, type, image: { id, caption } }
MarkAsRead { messaging_product, status, message_id }
```

3. Usar `#[serde(rename_all = "snake_case")]` donde aplique.
4. Usar `Option<T>` para campos que pueden no estar presentes.
5. Manejar el campo `type` con `#[serde(rename = "type")]` ya que `type` es keyword en Rust.

**Archivos a crear:**
- `src/whatsapp/mod.rs`
- `src/whatsapp/types.rs`

**Criterio de aceptación:**
- Los 4 payloads JSON de ejemplo de la sección 9.2 de la spec se deserializan correctamente sin errores.
- Los structs de salida se serializan al JSON exacto mostrado en la sección 9.3 de la spec.
- Escribir tests unitarios: al menos 1 test de deserialización por cada tipo de mensaje entrante (texto, botón, lista, imagen) y 1 test de serialización por cada tipo de mensaje saliente.

**NO hacer:**
- No definir structs para tipos de mensaje que el bot no usa (ubicación, contactos, audio, video, etc.).
- No usar `serde_json::Value` para parseo genérico. Cada campo debe tener su tipo.

---

### Paso 1.6 — Cliente de WhatsApp

**Qué hacer:**
1. Crear `src/whatsapp/client.rs` y `src/whatsapp/buttons.rs`.
2. Definir un struct `WhatsAppClient` que encapsule `reqwest::Client`, `whatsapp_token`, y `whatsapp_phone_id`.
3. Implementar los siguientes métodos async en `WhatsAppClient`:

```
send_text(to: &str, body: &str) -> Result<()>
send_buttons(to: &str, body: &str, buttons: Vec<Button>) -> Result<()>
send_list(to: &str, body: &str, button_text: &str, sections: Vec<ListSection>) -> Result<()>
send_image(to: &str, media_id: &str, caption: Option<&str>) -> Result<()>
mark_as_read(message_id: &str) -> Result<()>
```

4. Cada método hace POST a `https://graph.facebook.com/v21.0/{phone_id}/messages` con el payload correspondiente según sección 9.3 de la spec.
5. Loguear con `tracing::info!` cada mensaje enviado (destinatario y tipo).
6. Loguear con `tracing::error!` si la API de Meta responde con error.

7. En `buttons.rs` crear funciones builder para facilitar la construcción de botones y listas:
```
fn quick_buttons(body: &str, buttons: &[(&str, &str)]) -> OutgoingButtonMessage
// buttons es una lista de (id, title)

fn quick_list(body: &str, button_text: &str, rows: &[(&str, &str, &str)]) -> OutgoingListMessage
// rows es una lista de (id, title, description)
```

**Archivos a crear:**
- `src/whatsapp/client.rs`
- `src/whatsapp/buttons.rs`

**Criterio de aceptación:**
- Con un token y phone_id válidos (del número de prueba de Meta), `send_text()` envía un mensaje y se recibe en WhatsApp.
- `send_buttons()` envía un mensaje con botones interactivos correctamente renderizados.
- `send_list()` envía una lista interactiva correctamente renderizada.

**IMPORTANTE — Límite de 3 botones:**
- WhatsApp permite máximo 3 botones por mensaje interactivo.
- El menú principal tiene 4 opciones → DEBE usar lista interactiva, NO botones.
- Otros pasos con 3 o menos opciones pueden usar botones.

**NO hacer:**
- No implementar envío de media por URL. Solo por media_id (las imágenes se suben previamente a Meta).
- No implementar templates de mensaje (es para mejoras futuras).

---

### Paso 1.7 — Base de datos PostgreSQL

**Qué hacer:**
1. Crear `src/db/mod.rs`, `src/db/models.rs`, `src/db/queries.rs`.
2. Crear las 3 migraciones SQL exactamente como están en la sección 7 de la spec:
   - `migrations/001_create_conversations.sql`
   - `migrations/002_create_orders.sql`
   - `migrations/003_create_order_items.sql`
3. En `mod.rs`, crear función para inicializar el pool de conexiones `PgPool`.
4. En `models.rs`, definir los structs que mapean a las tablas:
   - `Conversation` con todos los campos de la tabla `conversations`.
   - `Order` con todos los campos de la tabla `orders`.
   - `OrderItem` con todos los campos de la tabla `order_items`.
5. En `queries.rs`, implementar las siguientes funciones:

```
get_conversation(pool, phone_number) -> Option<Conversation>
create_conversation(pool, phone_number) -> Conversation
update_state(pool, phone_number, state, state_data) -> Result<()>
update_customer_data(pool, phone_number, name, phone, address) -> Result<()>
update_last_message(pool, phone_number) -> Result<()>
create_order(pool, conversation_id, delivery_type, payment_method, total_estimated) -> Order
add_order_item(pool, order_id, flavor, has_liquor, quantity, unit_price, subtotal) -> OrderItem
update_order_status(pool, order_id, status) -> Result<()>
update_order_delivery_cost(pool, order_id, delivery_cost, total_final) -> Result<()>
reset_conversation(pool, phone_number) -> Result<()>
```

6. En `main.rs`, inicializar el pool y ejecutar migraciones al arrancar (`sqlx::migrate!().run(&pool)`).

**Archivos a crear:**
- `src/db/mod.rs`
- `src/db/models.rs`
- `src/db/queries.rs`
- `migrations/001_create_conversations.sql`
- `migrations/002_create_orders.sql`
- `migrations/003_create_order_items.sql`

**Criterio de aceptación:**
- Las migraciones se ejecutan sin errores contra una instancia de PostgreSQL.
- `create_conversation` crea un registro y `get_conversation` lo recupera correctamente.
- `update_state` cambia el estado y `state_data` de una conversación existente.

**NO hacer:**
- No crear tablas adicionales (como `customers` para clientes recurrentes — es mejora futura).
- No usar SQLite ni ningún otro motor. Solo PostgreSQL.

---

### Paso 1.8 — Integrar todo y parsear mensajes entrantes

**Qué hacer:**
1. En `main.rs`, crear un `AppState` (usando Axum's `State`) que contenga: `Config`, `PgPool`, `WhatsAppClient`.
2. En `webhook.rs`, después de validar HMAC:
   - Deserializar el body a `WebhookPayload`.
   - Extraer el primer mensaje del payload (si existe).
   - Determinar el tipo de input: `ButtonPress`, `TextMessage`, `ImageMessage`, o `ListSelection`.
   - Loguear: número del remitente, tipo de mensaje, contenido.
   - Por ahora responder con un echo: "Recibí tu mensaje: [contenido]" usando `send_text()`.
   - Marcar el mensaje como leído con `mark_as_read()`.
3. Si el payload no contiene mensajes (puede ser una notificación de status), ignorar silenciosamente y responder 200.

**Criterio de aceptación:**
- Enviar un texto al número de prueba → el bot responde con el echo.
- Presionar un botón → el bot responde con el ID del botón presionado.
- Enviar una imagen → el bot responde con "Recibí tu imagen".
- El mensaje queda marcado como leído (doble check azul en WhatsApp).

**NO hacer:**
- No implementar la máquina de estados todavía. Solo parsear y hacer echo.
- No consultar la base de datos todavía (eso es la Fase 2).

---

### Paso 1.9 — Dockerfile y deploy en Railway

**Qué hacer:**
1. Crear el `Dockerfile` exactamente como está en la sección 11 de la spec (multi-stage build).
2. Crear repositorio en GitHub, hacer push.
3. En Railway: crear proyecto, conectar repo de GitHub, agregar servicio PostgreSQL.
4. Configurar variables de entorno en Railway (todas las de la spec).
5. Railway construye y despliega automáticamente.
6. Configurar la URL de Railway como webhook en Meta for Developers.

**Archivos a crear:**
- `Dockerfile`

**Criterio de aceptación:**
- El deploy en Railway es exitoso (sin errores de build).
- El webhook de Meta se verifica correctamente (GET /webhook responde el challenge).
- Enviar un mensaje al número de prueba → el bot responde con el echo desde Railway.

**NO hacer:**
- No configurar dominio personalizado (no es necesario).
- No configurar CI/CD adicional (Railway ya hace deploy en cada push).

---

### ✅ VALIDACIÓN DE FASE 1

Antes de avanzar a la Fase 2, verificar que TODOS estos puntos se cumplen:

- [ ] `cargo build --release` compila sin warnings ni errores.
- [ ] El servidor arranca y escucha en el puerto configurado.
- [ ] GET /webhook verifica correctamente con Meta.
- [ ] POST /webhook valida firma HMAC y rechaza firmas inválidas.
- [ ] Mensajes de texto se parsean y el bot responde con echo.
- [ ] Botones interactivos se parsean (button_reply).
- [ ] Listas interactivas se parsean (list_reply).
- [ ] Imágenes se parsean (se extrae media_id).
- [ ] Mensajes se marcan como leídos.
- [ ] `send_text()` funciona.
- [ ] `send_buttons()` funciona (máximo 3 botones).
- [ ] `send_list()` funciona.
- [ ] PostgreSQL conectado y migraciones ejecutadas.
- [ ] CRUD básico de conversaciones funciona.
- [ ] Deploy en Railway exitoso y respondiendo.
- [ ] Logging visible en el dashboard de Railway.

---
---

## FASE 2: Máquina de estados básica

**Objetivo**: El cliente puede navegar el menú principal, registrar sus datos, y armar un pedido completo con selección de sabores y cantidades.

**Prerequisito**: Fase 1 completada y validada.

---

### Paso 2.1 — Enum de estados y tipos core

**Qué hacer:**
1. Crear `src/bot/mod.rs`, `src/bot/state_machine.rs`.
2. Implementar el enum `ConversationState` EXACTAMENTE como está en la sección 5.1 de la spec. Esto incluye los estados `ConfirmAddress` (confirmación de dirección despues de elegir forma de pago) y `WaitAdvisorConfirmHour` (esperar que el asesor confirme después de acordar hora). No agregar ni quitar variantes.
3. Implementar el enum `UserInput` como está en la sección 5.2 de la spec.
4. Implementar el enum `BotAction` como está en la sección 5.3 de la spec.
5. Implementar el trait `Serialize`/`Deserialize` para `ConversationState` para poder guardarlo en la columna `state` de PostgreSQL.
6. Implementar la función `extract_input(message: &IncomingMessage) -> UserInput` que convierte un mensaje entrante de WhatsApp en el tipo `UserInput` correspondiente.

**Archivos a crear:**
- `src/bot/mod.rs`
- `src/bot/state_machine.rs`

**Criterio de aceptación:**
- Todos los estados del mermaid tienen su variante en el enum.
- `ConversationState` se serializa/deserializa a string para PostgreSQL.
- `extract_input` convierte correctamente los 4 tipos de mensaje.

**NO hacer:**
- No implementar la función `transition()` todavía (se hace por partes en los siguientes pasos).
- No implementar los handlers de estados todavía.

---

### Paso 2.2 — Estructura de handlers y router de estados

**Qué hacer:**
1. Crear los archivos de handlers vacíos según la estructura de la spec sección 4:
   - `src/bot/states/mod.rs`
   - `src/bot/states/menu.rs`
   - `src/bot/states/scheduling.rs`
   - `src/bot/states/data_collect.rs`
   - `src/bot/states/order.rs`
   - `src/bot/states/checkout.rs`
   - `src/bot/states/advisor.rs`
   - `src/bot/states/relay.rs`
2. En `state_machine.rs`, implementar la función principal `transition()` como un `match` que delega a los handlers de cada módulo. **La función es PURA: no recibe wa_client ni pool. Solo retorna acciones que el executor se encarga de ejecutar.**

```rust
pub fn transition(
    state: &ConversationState,
    input: &UserInput,
    context: &mut ConversationContext,
) -> Result<(ConversationState, Vec<BotAction>)> {
    match state {
        ConversationState::MainMenu => menu::handle_main_menu(input, context).await,
        ConversationState::ViewMenu => menu::handle_view_menu(input, context).await,
        // ... etc, un match arm por cada estado
    }
}
```

3. Definir `ConversationContext` como struct que contiene los datos acumulados de la conversación (extraídos de `state_data` JSON):
```rust
pub struct ConversationContext {
    pub phone_number: String,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub delivery_address: Option<String>,
    pub items: Vec<OrderItemData>,
    pub delivery_type: Option<String>,
    pub scheduled_date: Option<String>,
    pub scheduled_time: Option<String>,
    pub payment_method: Option<String>,
    pub receipt_media_id: Option<String>,
}

pub struct OrderItemData {
    pub flavor: String,
    pub has_liquor: bool,
    pub quantity: u32,
}
```

**Archivos a crear:**
- `src/bot/states/mod.rs`
- `src/bot/states/menu.rs`
- `src/bot/states/scheduling.rs`
- `src/bot/states/data_collect.rs`
- `src/bot/states/order.rs`
- `src/bot/states/checkout.rs`
- `src/bot/states/advisor.rs`
- `src/bot/states/relay.rs`

**Criterio de aceptación:**
- `cargo check` compila sin errores con todos los handlers vacíos (retornando un placeholder).
- La función `transition()` tiene un match arm por CADA variante del enum `ConversationState`.

**NO hacer:**
- No implementar la lógica de los handlers todavía (solo la estructura y firmas).

---

### Paso 2.3 — Integrar máquina de estados con webhook

**Qué hacer:**
1. Modificar `webhook.rs` para que, al recibir un mensaje:
   - Determine si es del asesor (`from == ADVISOR_PHONE`) o de un cliente.
   - Si es un cliente: busque o cree su conversación en DB, cargue `ConversationContext` desde `state_data`, llame a `transition()`, ejecute las `BotAction`s resultantes, y guarde el nuevo estado en DB.
   - Si es del asesor: por ahora solo loguear (el manejo del asesor se implementa en Fase 4).
2. Implementar un executor de `BotAction` que recorra el `Vec<BotAction>` y ejecute cada acción:
   - `SendText` → `wa_client.send_text()`
   - `SendButtons` → `wa_client.send_buttons()`
   - `SendList` → `wa_client.send_list()`
   - `SendImage` → `wa_client.send_image()`
   - `ResetConversation` → `db::queries::reset_conversation()`
   - Los demás tipos (`StartTimer`, `CancelTimer`, `SaveOrder`, `RelayMessage`) → por ahora loguear "Action not implemented yet" (se implementan en fases posteriores).
3. Después de ejecutar las acciones, actualizar `last_message_at` en DB.

**Criterio de aceptación:**
- Un mensaje de un cliente nuevo crea una conversación en DB con estado `MainMenu`.
- El estado se persiste correctamente entre mensajes.
- Las acciones `SendText` y `SendButtons` se ejecutan y el cliente recibe los mensajes.

**NO hacer:**
- No implementar el manejo de mensajes del asesor todavía.
- No implementar timers todavía.

---

### Paso 2.4 — Menú principal y navegación

**Qué hacer:**
1. Implementar `menu::handle_main_menu()`:
   - Enviar una **lista interactiva** (NO botones, porque son 4 opciones y el límite es 3) con las opciones: Hacer Pedido, Ver Menú, Horarios, Hablar con Asesor.
   - Según la opción seleccionada, transicionar al estado correspondiente (ver mermaid nodo C).

2. Implementar `menu::handle_view_menu()`:
   - Enviar la imagen del menú con los sabores.
   - Enviar mensaje con todos los precios (detal y mayor) tal como aparece en el nodo F1 del mermaid.
   - Enviar botones: "Hacer Pedido" / "Volver al Menú".
   - Transicionar según la selección.

3. Implementar `menu::handle_view_schedule()`:
   - Enviar horarios tal como aparece en el nodo G1 del mermaid.
   - Enviar botones: "Hacer Pedido" / "Volver al Menú".
   - Transicionar según la selección.

**IMPORTANTE sobre el estado inicial:**
- Cuando un cliente escribe por primera vez (o su conversación está en `MainMenu`), el bot debe enviar un mensaje de bienvenida + la lista del menú principal.
- Si el cliente envía un texto que no corresponde a ningún botón/lista (ej: "hola"), tratarlo como nuevo y mostrar el menú principal.

**Archivos a modificar:**
- `src/bot/states/menu.rs`

**Criterio de aceptación:**
- Cliente escribe "hola" → recibe menú principal como lista interactiva con 4 opciones.
- Selecciona "Ver Menú" → recibe imagen + precios + botones.
- Selecciona "Horarios" → recibe horarios + botones.
- Desde Ver Menú o Horarios, puede volver al menú o ir a hacer pedido.

---

### Paso 2.5 — Flujo de tiempo de entrega

**Qué hacer:**
1. Implementar `scheduling::handle_when_delivery()` (nodo CUANDO del mermaid):
   - Enviar botones: "Entrega Inmediata" / "Entrega Programada".

2. Implementar `scheduling::handle_check_schedule()` (nodo VERIF_HOR):
   - Verificar la hora actual en timezone `America/Bogota` usando `chrono`.
   - Si está entre 8:00 AM y 11:00 PM → transicionar a `CollectName`.
   - Si no → transicionar a `OutOfHours`.

3. Implementar `scheduling::handle_out_of_hours()` (nodo FUERA_HOR):
   - Enviar mensaje de fuera de horario con botones: "Programar para después" / "Contactar asesor" / "Menú principal".

4. Implementar el flujo de programación (nodos PROG_FECHA, PROG_HORA, PROG_CONF):
   - `handle_select_date()`: Pedir al cliente que escriba la fecha (texto libre). Validar que sea una fecha válida y futura.
   - `handle_select_time()`: Pedir al cliente que escriba la hora. Validar formato.
   - `handle_confirm_schedule()`: Mostrar fecha y hora seleccionadas con botones "Confirmar" / "Cambiar". Si confirma → transicionar a `CollectName`.

**Archivos a modificar:**
- `src/bot/states/scheduling.rs`

**Criterio de aceptación:**
- "Entrega Inmediata" dentro de horario → avanza a recolección de datos.
- "Entrega Inmediata" fuera de horario → muestra mensaje de fuera de horario con opciones.
- "Entrega Programada" → permite seleccionar fecha y hora, confirmar, y avanzar.
- Fecha inválida o pasada → pide de nuevo con mensaje de error.

---

### Paso 2.6 — Recolección de datos del cliente

**Qué hacer:**
1. Implementar en `data_collect.rs` los 3 pasos de recolección (nodos R1, R2, R3 del mermaid):
   - `handle_collect_name()`: Enviar "¿Nombre completo?". Esperar texto. Validar que no esté vacío y tenga al menos 3 caracteres. Guardar en `ConversationContext.customer_name`.
   - `handle_collect_phone()`: Enviar "¿Teléfono de contacto?". Esperar texto. Validar que sean solo dígitos y tenga 7-15 caracteres. Guardar en `ConversationContext.customer_phone`.
   - `handle_collect_address()`: Enviar "¿Dirección de entrega?". Esperar texto. Validar que tenga al menos 10 caracteres. Guardar en `ConversationContext.delivery_address`.
2. Después de recolectar la dirección, transicionar a `SelectType`.
3. Guardar los datos del cliente en la tabla `conversations` con `update_customer_data()`.

**Archivos a modificar:**
- `src/bot/states/data_collect.rs`

**Criterio de aceptación:**
- El bot pide nombre → teléfono → dirección en secuencia.
- Datos inválidos (nombre vacío, teléfono con letras, dirección muy corta) → el bot pide de nuevo con mensaje de error claro.
- Después de la dirección, avanza a selección de granizados.
- Los datos quedan guardados en la DB.

---

### Paso 2.7 — Selección de granizados

**Qué hacer:**
1. Implementar en `order.rs` el flujo completo de selección (nodos TIPO_GRAN, SAB_LICOR, SAB_SIN, CANT_LIC, CANT_SIN, AGREGAR, MAS del mermaid):

   - `handle_select_type()`: Enviar botones "🍹 Con Licor" / "🧊 Sin Licor".
   - `handle_select_flavor()`: Según si es con o sin licor, enviar la imagen del menú correspondiente y pedir al cliente que escriba el sabor deseado. Guardar `has_liquor` en el estado.
   - `handle_select_quantity()`: Pedir "¿Cuántos de [sabor] [con/sin licor]?". Esperar número. Validar que sea entero positivo (1-999). Guardar el item en `ConversationContext.items`.
   - `handle_add_more()`: Enviar "✅ Agregado al pedido" + mostrar resumen parcial de items actuales + botones "Agregar más" / "Finalizar pedido".

2. Si el cliente elige "Agregar más" → volver a `SelectType`.
3. Si elige "Finalizar pedido" → transicionar a `ShowSummary`.

**IMPORTANTE sobre los sabores:**
- El bot envía una IMAGEN del menú de sabores y el cliente escribe el nombre del sabor como texto libre.
- No es necesario validar que el sabor exista en una lista predefinida (el asesor verifica después).
- Sí normalizar: trim, lowercase, para evitar duplicados por capitalización.

**Archivos a modificar:**
- `src/bot/states/order.rs`

**Criterio de aceptación:**
- El cliente puede seleccionar tipo → sabor → cantidad → agregar más → repetir.
- Cada item se acumula en `ConversationContext.items`.
- El resumen parcial muestra todos los items agregados hasta el momento.
- Cantidad no numérica o <= 0 → pide de nuevo.
- Puede agregar múltiples sabores de diferentes tipos (algunos con licor, otros sin).

---

### ✅ VALIDACIÓN DE FASE 2

- [ ] Cliente escribe "hola" → recibe menú principal (lista interactiva, 4 opciones).
- [ ] Navega Ver Menú → recibe imagen + precios → puede volver o hacer pedido.
- [ ] Navega Horarios → recibe horarios → puede volver o hacer pedido.
- [ ] Elige "Hacer Pedido" → elige entrega inmediata (dentro de horario) → avanza.
- [ ] Fuera de horario → recibe opciones correctas.
- [ ] Entrega programada → puede seleccionar fecha/hora y confirmar.
- [ ] Recolección: nombre, teléfono, dirección con validaciones.
- [ ] Selección: tipo → sabor → cantidad → agregar más (loop completo).
- [ ] Puede agregar 3+ items de tipos mixtos (con y sin licor).
- [ ] El estado se persiste correctamente entre mensajes (reiniciar el bot no pierde el progreso).
- [ ] Todos los datos se guardan en PostgreSQL.

---
---

## FASE 3: Precios, resumen y pago

**Objetivo**: El pedido se completa con cálculo automático de precios (incluyendo promo y clasificación detal/mayor), resumen con forma de pago integrada, y flujo de comprobante.

**Prerequisito**: Fase 2 completada y validada.

---

### Paso 3.1 — Módulo de precios

**Qué hacer:**
1. Crear `src/bot/pricing.rs`.
2. Implementar las 3 funciones de cálculo EXACTAMENTE como están en la sección 6 de la spec:
   - `calcular_precio_licor_detal(cantidad: u32) -> u32` — promo de pares.
   - `calcular_precio_sin_licor_detal(cantidad: u32) -> u32` — $7.000 fijo.
   - `precio_unitario_mayor(cantidad: u32, has_liquor: bool) -> u32` — tabla de rangos.
3. Implementar función que clasifica y calcula el pedido completo:
```rust
pub fn calcular_pedido(items: &[OrderItemData]) -> PedidoCalculado {
    // 1. Sumar total con licor y total sin licor por separado
    // 2. Clasificar cada tipo como detal (<20) o mayor (>=20)
    // 3. Aplicar precios correspondientes
    // 4. Retornar desglose + total
}

pub struct PedidoCalculado {
    pub items_detalle: Vec<ItemCalculado>,
    pub total_con_licor: u32,
    pub total_sin_licor: u32,
    pub es_mayor_con_licor: bool,
    pub es_mayor_sin_licor: bool,
    pub total_estimado: u32,
}
```
4. Escribir tests unitarios para TODOS los casos de la tabla de la sección 6.1 de la spec (1, 2, 3, 4, 5 unidades con licor).
5. Escribir tests para los rangos de mayor (20, 49, 50, 99, 100 unidades).
6. Escribir test para pedido mixto: ej. 3 con licor (detal) + 25 sin licor (mayor).

**Archivos a crear:**
- `src/bot/pricing.rs`

**Criterio de aceptación:**
- Todos los tests pasan.
- 2 con licor detal → $12.000 (no $16.000).
- 3 con licor detal → $20.000.
- 25 con licor mayor → 25 × $4.900 = $122.500.
- Pedido mixto calcula cada tipo independientemente.

**NO hacer:**
- No incluir el costo del domicilio en el cálculo (lo define el asesor después).

---

### Paso 3.2 — Confirmación de dirección, resumen y forma de pago

**Qué hacer:**
1. Implementar en `checkout.rs` el estado `ShowSummary` (nodo RESUMEN del mermaid):
   - Al entrar a este estado (desde `AddMore`), calcular el pedido con `calcular_pedido()`.
   - Construir un mensaje de texto con el resumen completo: cada item con sabor, tipo, cantidad, subtotal. Indicar si la promo aplicó. Mostrar total estimado + nota sobre domicilio.
   - Enviar el resumen como texto + **lista interactiva** (NO botones, porque hay 4 opciones y el límite de WhatsApp es 3 botones). Las opciones de la lista son: "💵 Contra Entrega", "📲 Pago Ahora", "✏️ Modificar pedido", "❌ Cancelar pedido".

2. Manejar las 4 opciones:
   - "Contra Entrega" → transicionar a `ConfirmAddress`.
   - "Pago Ahora" → enviar datos bancarios de donde cliente debe enviar dinero (texto con info de cuenta) → transicionar a `WaitReceipt`.
   - "Modificar" → volver a `SelectType` (mantener datos del cliente, limpiar items).
   - "Cancelar" → resetear conversación → `MainMenu`.

3. Implementar el estado `ConfirmAddress` (nodo PAGO_CONF del mermaid). **IMPORTANTE**: Este estado se alcanza desde `AddMore` ("Finalizar pedido"), DESPUES de `ShowSummary`. El flujo es: ShowSummary → (pago) → ConfirmAddress → envio al asesor:
   - Enviar al cliente: "Su dirección de entrega es: [dirección]. ¿Es correcta?" + botones "✅ Sí, correcta" / "✏️ Cambiar dirección".
   - Si confirma → transicionar al envio del resumen al asesor (Fase 4) (que calcula precios y muestra resumen).
   - Si cambia → pedir nueva dirección (texto libre) → actualizar en contexto y DB → volver a `ConfirmAddress`.

**Archivos a modificar:**
- `src/bot/states/checkout.rs`

**Criterio de aceptación:**
- El resumen muestra correctamente los precios calculados.
- La promo de pares se refleja en el subtotal (no precio unitario × cantidad simple).
- Las 4 opciones funcionan y transicionan correctamente.

---

### Paso 3.3 — Flujo de comprobante con timer

**Qué hacer:**
1. Implementar `src/bot/timers.rs` con la infraestructura de timers según sección 8 de la spec:
   - `TimerType` enum.
   - `TimerMap` como `Arc<Mutex<HashMap<TimerKey, CancellationToken>>>`.
   - Función `start_timer()`.
   - Función `cancel_timer()`.
2. Agregar `TimerMap` al `AppState` de Axum.
3. Implementar el estado `WaitReceipt`:
   - Al entrar: mensaje "Envíe una foto del comprobante" + iniciar timer de 10 minutos.
   - Si recibe imagen → cancelar timer → guardar `receipt_media_id` en contexto → transicionar a envío del resumen al asesor (por ahora transicionar a `ConfirmAddress` (confirmar dirección antes de enviar al asesor en Fase 4)).
   - Si expira timer → enviar "No recibimos el comprobante" + botones "Cambiar forma de pago" / "Cancelar".
   - "Cambiar forma de pago" → volver a `ShowSummary`.
   - "Cancelar" → resetear → `MainMenu`.
4. Implementar la recuperación de timers al arrancar el servidor según sección 8.3 de la spec.

**Archivos a crear/modificar:**
- `src/bot/timers.rs` (crear)
- `src/bot/states/checkout.rs` (modificar)
- `src/main.rs` (agregar TimerMap al AppState)

**Criterio de aceptación:**
- Cliente elige "Pago Ahora" → recibe datos bancarios → tiene 10 min para enviar imagen.
- Envía imagen dentro del tiempo → comprobante registrado, timer cancelado.
- No envía imagen en 10 min → recibe timeout con opciones.
- Si el servidor se reinicia con un timer pendiente, el timer se recrea correctamente.

---

### ✅ VALIDACIÓN DE FASE 3

- [ ] Pedido de 2 con licor → resumen muestra $12.000 (promo aplicada).
- [ ] Pedido de 3 con licor → resumen muestra $20.000.
- [ ] Pedido mixto (detal + mayor) → precios correctos por separado.
- [ ] "Contra Entrega" → muestra dirección, pide confirmación → "Sí, correcta" avanza, "Cambiar dirección" permite editar.
- [ ] "Pago Ahora" → después de enviar comprobante, también pasa por ConfirmAddress.
- [ ] "Pago Ahora" → después de enviar comprobante, también pasa por ConfirmAddress.
- [ ] "Pago Ahora" → muestra datos bancarios, espera comprobante.
- [ ] Enviar imagen como comprobante → se registra correctamente.
- [ ] Timer de 10 min expira → timeout con opciones.
- [ ] "Modificar" → vuelve a selección de sabores (datos del cliente se mantienen).
- [ ] "Cancelar" → vuelve al menú principal (todo limpio).
- [ ] `pricing.rs` tiene tests unitarios para todos los casos.

---
---

## FASE 4: Interacción con asesor

**Objetivo**: El asesor recibe pedidos, confirma, negocia hora, define domicilio, y puede atender clientes en modo relay. Todo a través del bot.

**Prerequisito**: Fase 3 completada y validada.

---

### Paso 4.1 — Identificar mensajes del asesor

**Qué hacer:**
1. En `webhook.rs`, implementar la lógica de bifurcación:
   - Si `from == config.advisor_phone` → delegar a `handle_advisor_message()`.
   - Si `from != config.advisor_phone` → delegar a `handle_client_message()` (lo que ya existe).
2. Crear función `handle_advisor_message()` que:
   - Extraiga el `button_id` si es un botón presionado.
   - Del `button_id`, extraiga el `phone_number` del cliente al que se refiere (los IDs tienen formato `action_phonenumber`, ej: `confirm_573001234567`).
   - Busque la conversación del cliente en DB.
   - Llame a `transition()` con el estado actual de esa conversación y el input del asesor.

**Archivos a modificar:**
- `src/routes/webhook.rs`

**Criterio de aceptación:**
- Mensajes del asesor se identifican correctamente.
- El bot extrae el phone_number del cliente del button_id.

---

### Paso 4.2 — Envío de resumen al asesor (ruta detal)

**Qué hacer:**
1. Implementar en `advisor.rs` la transición desde el pago confirmado hacia el asesor (nodos RESUMEN_FINAL → NOTIF → NOTIF_CLI del mermaid):
   - Construir resumen completo con: datos del cliente, detalle de items, total, método de pago, comprobante (si aplica), tipo de entrega, dirección.
   - **REGLA DE 4 DÍGITOS**: Cada mensaje al asesor debe incluir los últimos 4 dígitos del teléfono del cliente como identificador visual: "📋 Pedido [...4567]".
   - Enviar el resumen al asesor (`ADVISOR_PHONE`) como texto.
   - Enviar botones al asesor: "✅ Confirmar [...4567]" / "❌ No puedo [...4567]" / "🕐 Proponer hora [...4567]".
   - Los button_ids DEBEN incluir el phone completo del cliente: `confirm_573001234567`, `cannot_573001234567`, `propose_573001234567`.
   - Enviar mensaje al cliente: "Tu pedido ha sido enviado. Estamos confirmando disponibilidad..."
   - Iniciar timer de 2 minutos.
2. Enviar al asesor si el pedido es todo detal. Si incluye al mayor → se maneja en Paso 4.5.

**Archivos a modificar:**
- `src/bot/states/advisor.rs`

**Criterio de aceptación:**
- Al completar pago, el asesor recibe resumen + botones en su WhatsApp personal.
- El cliente recibe mensaje de espera.
- El timer de 2 min se inicia.

---

### Paso 4.3 — Respuesta del asesor: confirmar + domicilio

**Qué hacer:**
1. Implementar la respuesta del asesor al presionar "✅ Confirmar":
   - Cancelar timer de 2 min.
   - Enviar al asesor: "Pedido [...4567] confirmado. ¿Cuánto cobra de domicilio para [dirección]?"
   - Transicionar la conversación del cliente a `AskDeliveryCost`.
2. Cuando el asesor responde con el valor (texto libre, ej: "5000"):
   - Parsear como número. Si no es número válido, pedir de nuevo: "Por favor envíe solo el valor numérico del domicilio para [...4567] (ej: 5000)".
   - Calcular total final = total estimado + domicilio.
   - Guardar en DB: `update_order_delivery_cost()`.
   - Enviar al cliente el mensaje de confirmación final (nodo CONFIRM_DET del mermaid): "¡Pedido confirmado! ✅" con subtotal, domicilio, total final, tiempo estimado.
   - Transicionar a `OrderComplete` → resetear conversación a `MainMenu`.

**Archivos a modificar:**
- `src/bot/states/advisor.rs`

**Criterio de aceptación:**
- Asesor confirma → bot le pregunta domicilio.
- Asesor escribe "5000" → cliente recibe confirmación con total final correcto.
- El pedido en DB tiene status "confirmed", delivery_cost y total_final correctos.

---

### Paso 4.4 — Negociación de hora

**Qué hacer:**
1. Implementar respuesta del asesor al presionar "❌ No puedo":
   - Cancelar timer de 2 min.
   - Enviar al asesor: "Pedido [...4567]. ¿A qué hora puede?" (texto libre).
   - Transicionar a `NegotiateHour`.
2. Asesor escribe hora → enviar al cliente: "El asesor propone entrega a las [HORA]. ¿Acepta?" + botones "✅ Acepta" / "❌ Rechaza".
3. Cliente acepta → transicionar a `WaitAdvisorConfirmHour` → enviar al asesor "Cliente [...4567] aceptó la hora. ¿Confirma el pedido?" + botón "✅ Confirmar [...4567]" → cuando el asesor presiona confirmar → flujo de domicilio (Paso 4.3).
4. Cliente rechaza → pedir al cliente su hora → enviar al asesor "Cliente [...4567] propone las [HORA]. ¿Puede?" + botones "✅ Sí, confirmo [...4567]" / "🔄 Otra hora [...4567]".
5. El ciclo se repite hasta que ambos acuerden.

**Todos los botones del asesor llevan [...4567] en el título y el phone completo en el button_id.**

**Seguir EXACTAMENTE el flujo de los nodos ASK_HOUR → OFFER_HOUR → OFFER_ASE_OK → ASK_DOMICILIO y OFFER_HOUR → CLI_HORA → ENVIAR_ASE → ASE_DECIDE del mermaid.**

**Archivos a modificar:**
- `src/bot/states/advisor.rs`

**Criterio de aceptación:**
- El ciclo de negociación funciona: asesor propone → cliente acepta/rechaza → si rechaza, propone su hora → asesor acepta o contrapropone.
- Al aceptar, se procede a confirmación de domicilio.

---

### Paso 4.5 — Ruta al mayor: modo relay

**Qué hacer:**
1. Implementar en `relay.rs` el flujo completo de los nodos NOTIF_MAY → WAIT_MAY → RELAY_INIT → RELAY_MODE → RELAY_END del mermaid.

   **IMPORTANTE**: `RELAY_INIT` NO es un estado separado. Es la **acción de entrada** al transicionar a `RelayMode`. Al entrar en `RelayMode`, se ejecutan los mensajes de inicio y luego el estado queda en `RelayMode`.

   - Al detectar que el pedido incluye mayor (es_mayor_con_licor || es_mayor_sin_licor):
     - Enviar resumen al asesor con botón "✅ Tomar pedido [...4567]".
     - Enviar al cliente "Su pedido al por mayor ha sido enviado..."
     - Iniciar timer 2 min.
   - Asesor presiona "Tomar pedido":
     - Cancelar timer.
     - Enviar mensaje a ambos informando que el relay está activo (acción de entrada de RelayMode).
     - Transicionar a `RelayMode`.
   - Durante `RelayMode`:
     - Mensaje del cliente → reenviar al asesor con prefijo `[CLIENTE ...4567]:`
     - Mensaje del asesor (texto, no botón Finalizar) → reenviar al cliente sin prefijo.
     - Cada mensaje reinicia el timer de 30 min de inactividad.
   - Asesor presiona "🔴 Finalizar" o expira timer de 30 min:
     - Enviar cierre al cliente.
     - Transicionar a `OrderComplete` → resetear a `MainMenu`.
2. El botón "🔴 Finalizar [...4567]" debe enviarse al asesor como un mensaje separado después de cada relay, para que siempre esté visible.

**Archivos a modificar:**
- `src/bot/states/relay.rs`

**Criterio de aceptación:**
- Pedido al mayor → asesor recibe notificación → toma el pedido → relay activo.
- Mensajes del cliente se reenvían al asesor y viceversa.
- Asesor presiona Finalizar → relay termina limpiamente.
- 30 min sin mensajes → relay termina automáticamente.

---

### Paso 4.6 — Timeout del asesor

**Qué hacer:**
1. Implementar la expiración del timer de 2 min en los dos contextos (detal y mayor):
   - Para detal (nodo TIMEOUT): enviar al cliente "Asesor ocupado" + botones "📅 Programar" / "🔄 Reintentar" / "🏠 Menú".
   - Para mayor (nodo TIMEOUT_MAY): enviar al cliente lo mismo + mismas opciones.
   - "Programar" → ir a `SelectDate`.
   - "Reintentar" → reenviar el resumen al asesor y reiniciar timer de 2 min.
   - "Menú" → resetear → `MainMenu`.

**Archivos a modificar:**
- `src/bot/states/advisor.rs`

**Criterio de aceptación:**
- Timer de 2 min expira → cliente recibe opciones.
- Reintentar → se envía de nuevo al asesor.
- Programar → va al flujo de fecha/hora.

---

### Paso 4.7 — Opción "Hablar con Asesor" del menú

**Qué hacer:**
1. Implementar el flujo completo de los nodos H → H1 → H2 → H3 → H4 → H4_WAIT del mermaid:
   - Pedir nombre y teléfono al cliente (si no los tiene ya).
   - Enviar notificación al asesor: "Cliente quiere hablar" + datos + botones "✅ Atender [phone]" / "❌ No disponible [phone]".
   - Enviar al cliente "Contactando al asesor..."
   - Timer de 2 min.
2. Si el asesor presiona "Atender" → iniciar Modo Relay (reusar la lógica del Paso 4.5).
3. Si timeout o "No disponible":
   - Ofrecer "Dejar mensaje" / "Menú".
   - Si deja mensaje → pedir texto → enviar al asesor como notificación con contacto del cliente → volver al menú.

**Archivos a modificar:**
- `src/bot/states/advisor.rs`

**Criterio de aceptación:**
- "Hablar con Asesor" → pide datos → notifica asesor → asesor atiende → relay.
- Timeout → cliente puede dejar mensaje que llega al asesor.

---

### Paso 4.8 — Pedidos programados: gestión manual

**Qué hacer:**
1. Implementar la ruta de pedidos programados después de confirmación del asesor (nodo PROG_WAIT del mermaid):
   - Si el pedido es programado y el asesor confirma → enviar al cliente "Su pedido programado ha sido registrado. El asesor le confirmará en su momento."
   - Transicionar a `OrderComplete`.
2. Si el asesor no puede atender un pedido programado, el flujo es el mismo que para inmediato (negociar hora o timeout).

**Archivos a modificar:**
- `src/bot/states/advisor.rs`

---

### ✅ VALIDACIÓN DE FASE 4

- [ ] Pedido detal confirmado → asesor recibe resumen + botones con [...4567] en los títulos.
- [ ] Los button_ids incluyen el phone completo del cliente (ej: `confirm_573001234567`).
- [ ] Asesor confirma → pregunta domicilio mencionando [...4567] → digita valor → cliente recibe total final.
- [ ] Asesor "no puede" → propone hora → ciclo de negociación funciona, con [...4567] en cada mensaje.
- [ ] Cliente acepta hora → transiciona a `WaitAdvisorConfirmHour` → asesor recibe "¿Confirma?" → asesor confirma → domicilio.
- [ ] Timer 2 min expira → cliente recibe opciones (programar, reintentar, menú).
- [ ] Reintentar → se envía de nuevo al asesor.
- [ ] Pedido al mayor → asesor toma pedido → relay activo.
- [ ] En relay: mensajes del cliente se reenvían con prefijo `[CLIENTE ...4567]:`.
- [ ] En relay: mensajes del asesor se reenvían al cliente SIN prefijo.
- [ ] Asesor finaliza relay → conversación termina limpiamente → estado vuelve a `MainMenu`.
- [ ] 30 min de inactividad en relay → termina automáticamente → estado vuelve a `MainMenu`.
- [ ] "Hablar con Asesor" → relay o dejar mensaje. Botones del asesor incluyen [...4567].
- [ ] Múltiples conversaciones pendientes del asesor → los button_ids identifican cada cliente correctamente.
- [ ] El bot no se confunde si el asesor tiene 2 pedidos pendientes simultáneamente.
- [ ] `OrderComplete` resetea la conversación a `MainMenu` (no se persiste como estado).

---
---

## FASE 5: Validaciones, pulido y lanzamiento

**Objetivo**: Bot listo para producción, robusto ante inputs inesperados.

**Prerequisito**: Fase 4 completada y validada.

---

### Paso 5.1 — Manejo de inputs inesperados

**Qué hacer:**
1. En CADA estado del match de `transition()`, agregar un brazo `_ =>` que maneje inputs inesperados:
   - Si se espera un botón y el cliente envía texto → responder "Por favor selecciona una de las opciones:" y reenviar los botones.
   - Si se espera texto y el cliente envía un botón → responder "Por favor escribe tu respuesta:" y repetir la pregunta.
   - Si el cliente envía una imagen cuando no se espera → responder "No esperaba una imagen en este momento" y repetir la pregunta.
2. En ningún caso un input inesperado debe romper el flujo o dejar la conversación en un estado inconsistente.

**Criterio de aceptación:**
- En cualquier estado, enviar un tipo de input inesperado NO causa error y el bot se recupera.

---

### Paso 5.2 — Validaciones de input robustas

**Qué hacer:**
1. Revisar y reforzar todas las validaciones de texto libre:
   - **Nombre**: mínimo 3 caracteres, solo letras y espacios, trim.
   - **Teléfono**: solo dígitos, 7-15 caracteres, trim.
   - **Dirección**: mínimo 10 caracteres, trim.
   - **Sabor**: no vacío, trim, lowercase.
   - **Cantidad**: entero positivo, 1-999.
   - **Hora programada**: formato reconocible (ej: "3pm", "15:00", "3:00 PM").
   - **Fecha programada**: formato reconocible, fecha futura, no más de 30 días.
   - **Costo de domicilio** (asesor): entero positivo, rango 3,500-8,000.
2. Cada validación fallida debe dar un mensaje claro de qué está mal y qué se espera.

---

### Paso 5.3 — Envío de imágenes del menú

**Qué hacer:**
1. Crear `src/whatsapp/media.rs`.
2. Subir las imágenes del menú de sabores a Meta usando la Media API:
   - POST a `https://graph.facebook.com/v21.0/{phone_id}/media` con el archivo.
   - Guardar los `media_id` retornados como constantes en el código (o en variables de entorno).
3. Necesitas 3 imágenes:
   - Menú completo (para "Ver Menú").
   - Menú de sabores con licor (para `SelectFlavor` con licor).
   - Menú de sabores sin licor (para `SelectFlavor` sin licor).
4. En los estados correspondientes, usar `send_image()` con el `media_id` correcto.

**NOTA**: Las imágenes del menú deben ser proporcionadas por el dueño del negocio. Si no están disponibles, usar `send_text()` con la lista de sabores como texto como fallback temporal.

**Archivos a crear:**
- `src/whatsapp/media.rs`

---

### Paso 5.4 — Testing integral

**Qué hacer:**
Simular estos escenarios completos de principio a fin:

1. **Pedido detal inmediato con pago contra entrega**: Cliente → menú → hacer pedido → inmediata → datos → 2 con licor maracuyá + 1 sin licor mora → resumen ($12.000 + $7.000 = $19.000) → contra entrega → asesor confirma → domicilio $5.000 → total final $24.000 → pedido completo.

2. **Pedido detal inmediato con transferencia**: Mismo flujo pero con pago ahora → envía comprobante → asesor confirma → domicilio → total final.

3. **Pedido al mayor**: Cliente → 25 con licor → resumen ($122.500) → pago → asesor toma pedido → relay → asesor finaliza.

4. **Pedido mixto (detal + mayor)**: 3 con licor (detal: $20.000) + 30 sin licor (mayor: 30 × $4.800 = $144.000) = $164.000. Incluye mayor → ruta relay.

5. **Negociación de hora**: Asesor "no puede" → propone 4pm → cliente rechaza → cliente propone 5pm → asesor acepta → domicilio → confirmación.

6. **Timeouts**: Asesor no responde en 2 min → cliente programa → flujo completo.

7. **Comprobante timeout**: Cliente elige pago ahora → no envía comprobante → 10 min → cambia a contra entrega → completa.

8. **Hablar con asesor**: Cliente → menú → hablar con asesor → datos → asesor atiende → relay → asesor finaliza.

9. **Fuera de horario**: A las 11:30 PM → fuera de horario → programa para mañana → completa.

10. **Abandono y nuevo pedido**: Cliente llega hasta selección de sabor → no responde → escribe de nuevo después → ve menú principal → puede hacer nuevo pedido.

---

### Paso 5.5 — Logging y monitoreo

**Qué hacer:**
1. Verificar que cada transición de estado se loguea: `tracing::info!("phone={} state={} -> new_state={}", ...)`.
2. Verificar que cada mensaje enviado se loguea: `tracing::info!("sent {} to {}", type, to)`.
3. Verificar que cada error se loguea: `tracing::error!("failed to send message: {}", err)`.
4. Verificar que los timers se loguean al iniciar, cancelar, y expirar.
5. Verificar que todo es visible en el dashboard de logs de Railway.

---

### Paso 5.6 — Deploy final y migración del número

**Qué hacer:**
1. Verificar que todas las variables de entorno de producción están configuradas en Railway.
2. Hacer deploy final con `git push`.
3. Verificar en logs de Railway que el servidor arranca correctamente.
4. Migrar el número real de WhatsApp Business a la Cloud API (seguir pasos de la sección 3.2 del documento de negocio).
5. Actualizar `WHATSAPP_PHONE_ID` y `WHATSAPP_TOKEN` en Railway con los valores del número real.
6. Configurar webhook de Meta apuntando a la URL de Railway.
7. Hacer un pedido de prueba completo con el número real.

---

### ✅ VALIDACIÓN FINAL

- [ ] Los 10 escenarios de testing integral pasan.
- [ ] Inputs inesperados en cualquier estado se manejan sin errores.
- [ ] Validaciones de texto muestran mensajes claros.
- [ ] Imágenes del menú se envían correctamente (o fallback de texto).
- [ ] Logging completo visible en Railway.
- [ ] Deploy en Railway exitoso con el número real.
- [ ] Un pedido real de principio a fin funciona correctamente.
- [ ] El asesor practicó con el sistema y confirma que es usable.