# Referencia Operativa Actual

## Resumen

Este documento reemplaza los antiguos documentos de `general_info/phase_planning/`.
Su objetivo es describir el funcionamiento real y actual del bot en produccion, con el flujo vigente, la persistencia real, los timers activos, las dependencias operativas y la validacion practica del servicio.

Este archivo debe mantenerse alineado con:

- `AGENTS.md`
- `general_info/Flow_Design_Diagram_v2.mermaid`
- la implementacion vigente en `src/`
- `LICENSE`

Licenciamiento actual del repositorio:

- el repositorio es propietario y se distribuye bajo `All Rights Reserved`
- la visibilidad publica del codigo no concede permiso para copiarlo, modificarlo, redistribuirlo o venderlo
- solo se permite ver el codigo y ejecutar el simulator localmente para evaluacion personal no comercial, segun `LICENSE`

## Arquitectura Actual

El proyecto es un servicio Rust con dos modos de runtime:

- `BOT_MODE=production`: recibe webhooks de Meta Cloud API, valida la firma HMAC, clasifica los mensajes entre cliente y asesor, ejecuta una maquina de estados persistente y responde por WhatsApp usando texto, botones, listas e imagenes.
- `BOT_MODE=simulator`: expone una UI local en `/simulator`, procesa los mismos estados y timers, persiste el mismo flujo de conversaciones/pedidos y registra las salidas en transcriptos locales en vez de llamar Meta.

Componentes principales:

- `src/routes/`
  - `verify.rs`: verificacion de `GET /webhook`
  - `webhook.rs`: recepcion del webhook productivo y normalizacion de inputs
  - `simulator.rs`: UI local y endpoints JSON para manual testing completo sin Meta
  - `legal.rs`: paginas publicas `/privacy-policy` y `/terms-of-service`
- `src/engine.rs`
  - procesamiento compartido de cliente/asesor
  - ejecucion compartida de acciones para webhook, simulator y timers
- `src/whatsapp/`
  - cliente de Meta Cloud API
  - builders de botones y listas
  - tipos serde para payloads entrantes y salientes
- `src/simulator/`
  - sesiones locales
  - transcriptos persistidos
  - media local para comprobantes e imagenes
- `src/bot/`
  - maquina de estados
  - handlers por estado
  - logica de precios
  - timers y restauracion tras reinicio
- `src/db/`
  - modelos SQLx
  - queries de conversaciones y pedidos
- `migrations/`
  - esquema PostgreSQL

## Ruteo Real Del Webhook

Flujo base:

1. Meta llama `POST /webhook`.
2. El bot valida `X-Hub-Signature-256`.
3. Si la firma es valida, responde `200` de inmediato y procesa de forma asincrona.
4. Si el payload no trae mensajes entrantes, el bot solo registra el evento y no ejecuta flujo conversacional.
5. Si el `from` coincide con `ADVISOR_PHONE`, el mensaje entra siempre al flujo de asesor.
6. Cualquier otro numero entra al flujo de cliente.

Comportamiento actual relevante:

- `mark_as_read` es best-effort. Si Meta rechaza ese request, el bot solo registra warning y sigue procesando.
- Los logs del runtime priorizan visibilidad operativa con telefono enmascarado, previews cortos de texto, transiciones de estado, resumen de respuestas salientes y eventos de timers. Los callbacks de estado `sent/delivered/read` de Meta deben quedar fuera del ruido normal de `INFO` y verse en `DEBUG` cuando haga falta.
- El callback productivo exacto es `/webhook`.
- Para trafico publico real, la app de Meta debe estar en `Live` y el WABA debe estar suscrito a la app activa.

## Runtime Local Simulator

Cuando `BOT_MODE=simulator`:

- no se monta `/webhook`
- no se requieren credenciales `WHATSAPP_*`
- el servicio se liga por defecto a `127.0.0.1`
- la UI local vive en `/simulator`
- cada sesion local crea o reutiliza un cliente identificado por telefono y nombre de perfil opcional
- el bot sigue usando `conversations`, `orders`, `order_items`, restauracion de timers y sweep periodico
- las respuestas del bot se persisten en transcriptos locales en vez de salir a Meta
- los comprobantes o imagenes de prueba se guardan en disco local y se referencian por id local
- cada mensaje del transcript muestra su `created_at` en `America/Bogota`
- la UI expone timers activos con inicio, vencimiento, countdown y fase del timeout
- la UI permite overrides locales de timers solo para nuevas esperas creadas en simulator
- los timeouts del simulator registran avisos de sistema indicando si se dispararon por runtime, sweep o reconciliacion de arranque
- el repositorio incluye launchers en `scripts/` para arrancar el simulator con defaults razonables en macOS/Linux y Windows
- esos launchers pueden crear o arrancar un Postgres local via Docker si `DATABASE_URL` no fue configurado manualmente
- el simulator siempre usa `assets/trabix-menu.png` como imagen local rastreada para `Ver Menú`
- si quieres que otro equipo vea el menú real al clonar el repo, reemplaza ese archivo rastreado y súbelo a GitHub

El objetivo del simulator es validar localmente el mismo comportamiento productivo del bot, incluyendo:

- persistencia de `customer_name`, `customer_phone` y `delivery_address`
- preservacion de esos datos cuando la conversacion vuelve a `main_menu`
- flujo completo de cliente, asesor, relay, recibos y timers
- recuperacion de timers y transcriptos despues de reinicio

## Flujo Real Del Cliente

### Menu Principal

Estado inicial persistido: `main_menu`.

El bot responde con:

- mensaje de bienvenida
- horario de entrega inmediata: `8:00 AM` a `11:00 PM`
- 3 botones:
  - `Hacer Pedido`
  - `Ver Menú`
  - `Hablar con Asesor`

No existe ya un flujo principal separado de `Horarios`; cualquier conversacion legacy que aun rehidrate `view_schedule` debe reconducirse al menu actual.

### Ver Menu

`view_menu` envia:

- una unica imagen del menu usando `MENU_IMAGE_MEDIA_ID`
- texto de menu/precios desde `config/messages.toml`
- botones `Hacer Pedido` y `Volver al Menu`

La imagen del menu solo se envia en esta ruta. El runtime actual no usa imagenes separadas por sabor o por tipo con/sin licor.

### Hacer Pedido

`when_delivery` permite:

- `Entrega Inmediata`
- `Entrega Programada`

#### Entrega inmediata

- Si la hora de Bogota esta entre `08:00` y `23:00`, el flujo pasa a captura de datos.
- Si esta fuera de horario, el bot pasa a `out_of_hours` y ofrece:
  - programar entrega
  - hablar con asesor
  - volver al menu

#### Entrega programada

El flujo usa texto libre con validacion minima:

- `select_date`
- `select_time`
- `confirm_schedule`

La fecha y la hora programadas se conservan como texto en el contexto y tambien se persisten en `orders` como `scheduled_date_text` y `scheduled_time_text` cuando aplica.

### Captura De Datos Del Cliente

Al entrar un mensaje del cliente, el runtime intenta sembrar datos automaticamente desde el webhook:

- `customer_phone` desde `messages[].from`
- `customer_name` desde `contacts[].profile.name` cuando Meta lo incluye

Los datos manuales ya guardados no se sobreescriben con metadata nueva del webhook.

Luego el flujo pide solo lo que falte:

- `collect_name`
- `collect_phone`
- `collect_address`

Los datos del cliente se persisten en columnas de `conversations`:

- `customer_name`
- `customer_phone`
- `delivery_address`

### Armado Del Pedido

El loop actual es:

- `select_type`
- `select_flavor`
- `select_quantity`
- `add_more`

Comportamiento actual:

- primero se elige `Con Licor` o `Sin Licor`
- luego se muestra una lista de sabores compatible con WhatsApp
- luego se captura la cantidad
- luego se muestra un resumen parcial con botones para:
  - `Agregar más`
  - `Finalizar pedido`
  - `Reiniciar pedido`
- `Reiniciar pedido` pide confirmacion y elimina todos los items actuales antes de volver a `select_type`

La seleccion parcial vive en:

- `pending_has_liquor`
- `pending_flavor`

Los items finales se guardan en `state_data.items`.

## Checkout Y Pedido

### Resumen Y Pago

`show_summary` calcula el pedido con `src/bot/pricing.rs` y presenta:

- datos del cliente
- tipo de entrega
- fecha/hora si el pedido es programado
- items y subtotales
- total estimado sin domicilio
- nota de que el domicilio se define despues

Opciones actuales:

- `Contra Entrega`
- `Pago Ahora`
- `Cancelar Pedido`

### Pago Ahora

`wait_receipt`:

- envia instrucciones de transferencia
- espera una imagen de comprobante
- inicia timer de `10 minutos`

Comportamiento:

- solo acepta imagen como comprobante valido
- si llega texto u otro input, corrige y repite la instruccion
- si vence el timer:
  - marca `receipt_timer_expired = true`
  - ofrece `Cambiar pago` o `Cancelar`

### Revision De Datos Antes Del Handoff

`confirm_address`:

- muestra un resumen de:
  - nombre
  - telefono
  - direccion
- ofrece `Continuar` o `Cambiar`
- si el cliente elige `Cambiar`, entra a un selector para editar:
  - `Nombre`
  - `Teléfono`
  - `Dirección`
- despues de editar un campo, vuelve al mismo resumen antes del handoff al asesor

### Persistencia Del Pedido

Durante checkout y handoff, el bot usa `orders` y `order_items`.

Campos importantes de `orders`:

- `delivery_type`
- `scheduled_date`
- `scheduled_time`
- `scheduled_date_text`
- `scheduled_time_text`
- `payment_method`
- `receipt_media_id`
- `delivery_cost`
- `total_estimated`
- `total_final`
- `status`

Estados operativos relevantes de la orden:

- `draft_payment`
- `pending_advisor`
- `confirmed`
- `manual_followup`
- `cancelled`

`current_order_id` en `state_data` permite retomar el pedido en pasos posteriores sin ambiguedad.

## Flujo Real Del Asesor

### Regla De Ruteo

`ADVISOR_PHONE` nunca entra al flujo de cliente.

Si el asesor escribe sin haber seleccionado antes un caso pendiente, el bot responde con el mensaje de guia para el asesor y no muestra el menu de cliente.

### Pedido Normal Con Asesor

Despues de confirmar direccion, el pedido pasa a handoff.

Comportamiento actual:

- se calcula el pedido
- se crea o actualiza el borrador persistido
- se envia resumen al asesor
- si existe comprobante, tambien se envia la imagen al asesor
- el cliente queda en espera

Ramas actuales:

- pedido programado:
  - el asesor recibe boton de confirmar
- pedido inmediato:
  - el asesor recibe botones de confirmar o indicar que no puede

### Confirmacion De Pedido Inmediato

Ruta real:

- `wait_advisor_response`
- `ask_delivery_cost`
- cierre del pedido

El asesor digita el costo del domicilio y el bot:

- actualiza `delivery_cost`
- calcula `total_final`
- cambia la orden a `confirmed`
- informa al cliente el total final y el tiempo estimado
- resetea la conversacion a `main_menu`

### Negociacion De Hora

Si el asesor no puede atender un pedido inmediato en ese momento:

- el pedido se convierte operativamente en programado
- se negocia hora entre asesor y cliente

Estados relevantes:

- `negotiate_hour`
- `offer_hour_to_client`
- `wait_client_hour`
- `wait_advisor_hour_decision`
- `wait_advisor_confirm_hour`

Al confirmar la hora final:

- el pedido queda `confirmed`
- el cliente recibe confirmacion de pedido programado
- la conversacion vuelve a `main_menu`

### Hablar Con Asesor

La ruta `Hablar con Asesor`:

- usa los datos ya existentes del cliente si estan disponibles
- si falta nombre o telefono, los pide antes de contactar al asesor
- antes de entrar a `wait_advisor_contact`, muestra un resumen con nombre y telefono para `Continuar` o `Cambiar`
- si el cliente elige `Cambiar`, puede editar `Nombre` o `Teléfono` y luego vuelve al resumen

Estados:

- `contact_advisor_name`
- `contact_advisor_phone`
- `confirm_address` con alcance `advisor_contact`
- `wait_advisor_contact`
- `leave_message`

Ramas:

- si el asesor atiende, se entra a relay
- si el asesor no esta disponible o vence el timer, el cliente puede:
  - dejar mensaje
  - volver al menu

### Relay

El relay se usa para:

- contacto directo con asesor
- atencion de pedidos al por mayor

Comportamiento actual:

- cliente -> asesor: se reenvia con prefijo `[CLIENTE ...xxxx]:`
- asesor -> cliente: se reenvia como texto libre
- el asesor tiene boton `Finalizar`
- si el relay termina manualmente o por timeout, la conversacion del cliente se resetea a `main_menu`

`relay_kind` identifica el contexto del relay:

- `wholesale_order`
- `contact_advisor`

## Timers Activos

Timers de runtime:

- comprobante: `10 minutos`
- espera de asesor: `2 minutos`
- estados detallados del asesor atascados: `30 minutos`
- relay: `30 minutos`
- inactividad generica del cliente:
  - recordatorio a los `2 minutos`
  - reinicio a los `35 minutos`

### Inactividad Generica Del Cliente

La inactividad generica aplica solo a estados de entrada del cliente, por ejemplo:

- `main_menu`
- `view_menu`
- `when_delivery`
- `select_date`
- `collect_name`
- `select_type`
- `show_summary`
- `confirm_address`
- `select_customer_data_field`
- `edit_customer_name`
- `edit_customer_phone`
- `edit_customer_address`
- `contact_advisor_name`
- `leave_message`

No aplica a estados ya gobernados por timers propios, como:

- `wait_receipt`
- `wait_advisor_response`
- `wait_advisor_contact`
- `relay_mode`

Comportamiento actual:

- se arma solo por una interaccion real del cliente
- a los `2 minutos` reenvia el prompt actual una sola vez
- a los `35 minutos` envia mensaje de reinicio y resetea a `main_menu`
- despues de ese reinicio no debe volver a dispararse nada hasta que el cliente escriba de nuevo

### Reinicio Del Servicio

El bot restaura timers activos con `restore_pending_timers()`.

Comportamiento actual tras deploy o reinicio:

- timers aun vigentes:
  - se rearman con el tiempo restante
- timers ya vencidos:
  - se reconcilian de forma silenciosa en base de datos
  - no deben generar mensajes salientes por el simple hecho de que el proceso arrancó

Catch-up silencioso actual:

- `wait_receipt`: marca timeout pendiente sin enviar mensajes en boot
- `wait_advisor_response`, `wait_advisor_mayor`, `wait_advisor_contact`: marca timeout del asesor sin fanout en boot
- `ask_delivery_cost`, `negotiate_hour`, `wait_advisor_hour_decision`, `wait_advisor_confirm_hour`: puede resetear y mover orden a `manual_followup` sin enviar mensajes en boot
- `relay_mode`: cierra silenciosamente si ya estaba vencido
- inactividad generica:
  - si ya debia mandar recordatorio, marca el recordatorio como consumido y solo conserva el deadline de reset
  - si ya debia resetear, resetea silenciosamente

## Persistencia Real

### Tabla `conversations`

Campos importantes:

- `phone_number`
- `state`
- `state_data`
- `customer_name`
- `customer_phone`
- `delivery_address`
- `last_message_at`

`state` se persiste como string `snake_case`.

### `state_data`

Campos mas importantes hoy:

- `items`
- `delivery_type`
- `customer_review_scope`
- `scheduled_date`
- `scheduled_time`
- `payment_method`
- `receipt_media_id`
- `receipt_timer_started_at`
- `advisor_target_phone`
- `advisor_timer_started_at`
- `advisor_timer_expired`
- `relay_timer_started_at`
- `relay_kind`
- `advisor_proposed_hour`
- `client_counter_hour`
- `schedule_resume_target`
- `current_order_id`
- `editing_address`
- `receipt_timer_expired`
- `pending_has_liquor`
- `pending_flavor`
- `conversation_abandon_started_at`
- `conversation_abandon_reminder_sent`

### Tabla `order_items`

Cada item persistido guarda:

- `flavor`
- `has_liquor`
- `quantity`
- `unit_price`
- `subtotal`

## Configuracion Y Operacion

Variables y datos operativos clave:

- `BOT_MODE`
- `DATABASE_URL`
- `TEST_DATABASE_URL`
- `WHATSAPP_TOKEN`
- `WHATSAPP_PHONE_ID`
- `WHATSAPP_VERIFY_TOKEN`
- `WHATSAPP_APP_SECRET`
- `ADVISOR_PHONE`
- `MENU_IMAGE_MEDIA_ID`
- `SIMULATOR_UPLOAD_DIR`

Notas actuales:

- los mensajes del cliente viven en `config/messages.toml`
- `TRANSFER_PAYMENT_TEXT` queda como fallback legado si `config/messages.toml` no define el texto de transferencia
- las sesiones PostgreSQL del bot usan `America/Bogota`
- `FORCE_BOGOTA_NOW=YYYY-MM-DD HH:MM` es solo para pruebas locales de horario
- `WHATSAPP_TEST_RECIPIENT` sirve para smoke tests live, no define el numero productivo escuchado por el bot
- `BOT_MODE=simulator` no usa `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_ID`, `WHATSAPP_VERIFY_TOKEN`, `WHATSAPP_APP_SECRET` ni `MENU_IMAGE_MEDIA_ID`
- `assets/trabix-menu.png` es la imagen rastreada usada por `Ver Menú` en simulator
- `SIMULATOR_UPLOAD_DIR` guarda imagenes locales de prueba como comprobantes o capturas

Validaciones operativas importantes:

- confirmar `/{WABA_ID}/subscribed_apps`
- confirmar que la app Meta este en `Live`
- confirmar que Railway use un token permanente con acceso al mismo WABA y `WHATSAPP_PHONE_ID`
- confirmar que el callback exacto sea `/webhook`

## Validacion Actual

### Comandos Base

```bash
cargo check
cargo test
cargo test --test live_whatsapp -- --ignored --test-threads=1
cargo run --bin granizado-bot
BOT_MODE=simulator cargo run --bin granizado-bot
```

### Checklist Manual Minimo

Cliente:

- escribir a un chat nuevo y verificar menu principal actual
- navegar por `Ver Menú`
- crear pedido inmediato y programado
- validar captura de nombre, telefono y direccion
- validar loop de items y llegada a `ShowSummary`
- validar `Contra Entrega`
- validar `Pago Ahora` con imagen de comprobante
- validar cambio de direccion

Asesor:

- confirmar pedido inmediato y capturar domicilio
- negociar hora para un pedido que no puede salir inmediato
- validar que el asesor no puede responder sin seleccionar un caso pendiente
- validar multiples casos pendientes sin cruce de contexto

Simulator local:

- crear sesion local con telefono y nombre de perfil
- verificar que el primer mensaje siembre `customer_phone` y `customer_name`
- verificar que direccion, telefono y nombre persisten despues de reset a `main_menu`
- validar `Ver Menú` con imagen local
- validar `Pago Ahora` con imagen subida desde el navegador local
- reiniciar el servicio y confirmar recuperacion de transcriptos, estados y timers activos
- confirmar que nada sale a Meta durante la prueba

Relay y contacto:

- validar `Hablar con Asesor`
- validar rama `Atender`
- validar rama `No disponible`
- validar `Dejar mensaje`
- validar relay cliente -> asesor y asesor -> cliente
- validar `Finalizar`

Timers y reinicio:

- comprobar timeout de comprobante
- comprobar timeout de asesor con `Programar`, `Reintentar` y `Menu`
- comprobar hard reset de waits detallados del asesor
- comprobar timeout de relay
- comprobar recordatorio y reset de inactividad del cliente
- reiniciar el servicio con timers activos y verificar restauracion
- redeployar o reiniciar con timers ya vencidos y verificar que no se envien mensajes salientes por boot

Persistencia:

- revisar `conversations.state`
- revisar `conversations.state_data`
- revisar `orders`
- revisar `order_items`

## Mantenimiento Del Documento

Si cambia cualquiera de estos puntos, este documento debe actualizarse en el mismo ciclo de trabajo:

- flujo real cliente/asesor
- timers
- persistencia o `state_data`
- requisitos operativos de Meta/Railway/PostgreSQL
- validacion manual que se use como checklist vigente

Este archivo debe reemplazar por completo a la antigua narrativa por fases. Si una futura funcionalidad cambia el comportamiento productivo, se documenta aqui como estado actual, no como fase futura.
