# Referencia Operativa Actual

## Resumen

Este documento reemplaza los antiguos documentos de `general_info/phase_planning/`.
Su objetivo es describir el funcionamiento real y actual del bot en produccion, con el flujo vigente, la persistencia real, los timers activos, las dependencias operativas y la validacion practica del servicio.

Este archivo debe mantenerse alineado con:

- `AGENTS.md`
- `general_info/complex_diagram.mermaid`
- `general_info/simple_diagram.mermaid`
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
  - `simulator.rs`: wrapper minimo que monta el runtime del simulator
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
  - `web.rs`: handlers HTTP del simulator y serving de assets locales
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
- el frontend del simulator vive en `assets/simulator/index.html`, `assets/simulator/simulator.css` y `assets/simulator/simulator.js`
- el backend HTTP del simulator vive en `src/simulator/web.rs`
- cada sesion local crea o reutiliza un cliente identificado por telefono y nombre de perfil opcional
- el bot sigue usando `conversations`, `orders`, `order_items`, restauracion de timers y sweep periodico
- las respuestas del bot se persisten en transcriptos locales en vez de salir a Meta
- los comprobantes o imagenes de prueba se guardan en disco local y se referencian por id local
- cada mensaje del transcript muestra su `created_at` en `America/Bogota`
- el panel de asesor es por sesion; los mensajes del asesor y del bot para ese caso se ven dentro del chat de esa sesion
- la UI expone timers activos con inicio, vencimiento, countdown y fase del timeout
- la UI permite overrides locales de timers solo para nuevas esperas creadas en simulator
- los timeouts del simulator registran avisos de sistema indicando si se dispararon por runtime, sweep o reconciliacion de arranque
- la UI hace auto-refresh del transcript, el estado persistido y la vista de base de datos para que timers y mensajes aparezcan sin recargar manualmente
- la UI expone un inspector read-only de base de datos para `conversations`, `orders` y `order_items`
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

### Revision Final Antes Del Asesor

`review_checkout` calcula el pedido con `src/bot/pricing.rs` y presenta:

- datos del cliente
- tipo de entrega
- fecha/hora si el pedido es programado
- items y subtotales
- total estimado sin domicilio
- nota de que el domicilio se define antes del pago final

Opciones actuales:

- `Continuar`
- `Modificar datos`

Si el cliente elige modificar:

- entra a `select_customer_data_field`
- puede editar `Nombre`, `Teléfono`, o `Dirección`
- despues de editar, vuelve a `review_checkout`

### Pago Final

El pago ya no se elige antes del handoff.

Despues de la gestion del asesor:

- si el pedido no tiene ningun bucket al por mayor, el bot entra directamente a `select_payment_method`
- si el pedido si tiene al menos un bucket al por mayor, el bot entra primero a `select_referral_option`

### Referral Antes Del Pago

`select_referral_option` solo aparece para pedidos con pricing al por mayor.

El cliente ve:

- mensaje indicando que ese es el momento para usar codigo de descuento
- botones `Tengo código` y `Seguir sin código`

Si elige `Tengo código`:

- entra a `wait_referral_code`
- el bot espera texto libre
- normaliza el input con `trim().to_lowercase()`
- valida el codigo contra `config/referrals.toml`

Si el codigo es invalido:

- el bot sigue en `wait_referral_code`
- muestra botones `Reintentar código` y `Seguir sin código`

Si el codigo es valido:

- el descuento se aplica solo sobre los buckets ya calculados como `mayor`
- cada bucket elegible calcula su tier de forma independiente:
  - `20-49`: cliente `10%`, embajador `15%`
  - `50-99`: cliente `12%`, embajador `18%`
  - `100+`: cliente `15%`, embajador `20%`
- el domicilio no participa en el descuento ni en la comision
- el bot recalcula:
  - `referral_discount_total`
  - `ambassador_commission_total`
  - `total_final = subtotal_con_descuento + delivery_cost`
- el cliente recibe confirmacion del codigo aplicado
- el cliente vuelve a ver el resumen listo para pago con codigo, descuento, subtotal con descuento, domicilio y total final
- luego entra a `select_payment_method`

Si elige `Seguir sin código`:

- conserva los totales originales
- entra directo a `select_payment_method`

### Seleccion De Pago

`select_payment_method` muestra botones:

- `Contra Entrega`
- `Pago Ahora`

`Contra Entrega`:

- actualiza `payment_method = cash_on_delivery`
- confirma la orden
- envia al asesor el paquete final con resumen completo del pedido, datos del cliente y totales finales
- envia confirmacion final
- resetea la conversacion a `main_menu`

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
- si llega una imagen valida:
  - persiste `receipt_media_id`
  - envia al asesor el paquete final con resumen completo del pedido, datos del cliente y totales finales
  - reenvia el comprobante al asesor
  - confirma la orden
  - resetea la conversacion a `main_menu`

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
- `referral_code`
- `referral_discount_total`
- `ambassador_commission_total`
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

`state_data` tambien persiste:

- `referral_code`
- `referral_discount_total`
- `ambassador_commission_total`
- `delivery_cost`
- `total_final`
- contexto de pago y comprobante
- timers del asesor y del comprobante

## Flujo Real Del Asesor

### Regla De Ruteo

`ADVISOR_PHONE` nunca entra al flujo de cliente.

Si el asesor escribe sin haber seleccionado antes un caso pendiente, el bot responde con el mensaje de guia para el asesor y no muestra el menu de cliente.

### Pedido Normal Con Asesor

Despues de `review_checkout`, el pedido pasa al asesor.

Comportamiento actual:

- se calcula el pedido
- se crea o actualiza el borrador persistido con `payment_method = pending`
- se envia resumen al asesor
- el asesor primero digita el costo del domicilio en `ask_delivery_cost`
- al finalizar el pago, el asesor recibe un paquete final con el pedido ya confirmado y los totales definitivos

### Pedido Programado

Ruta real:

- `ask_delivery_cost`
- `select_referral_option` opcional solo si hay bucket al por mayor
- `select_payment_method`
- `wait_receipt` opcional

Despues de digitar el domicilio:

- se actualiza `delivery_cost`
- se calcula `total_final`
- la orden pasa a `draft_payment`
- el cliente recibe confirmacion del pedido programado con subtotal, domicilio y total final
- si el pedido aplica al por mayor, el cliente entra antes por la validacion opcional de referral
- no se espera un boton extra de confirmacion del asesor

### Pedido Inmediato

Ruta real:

- `ask_delivery_cost`
- `wait_advisor_response`
- `select_referral_option` opcional solo si hay bucket al por mayor
- `select_payment_method`
- `wait_receipt` opcional

Despues de digitar el domicilio:

- se actualiza `delivery_cost`
- se calcula `total_final`
- el asesor recibe solo el boton `Confirmar`
- si confirma, el cliente recibe subtotal, domicilio, total final y luego el paso opcional de referral antes del selector de pago cuando aplica al por mayor
- si el asesor no responde durante `5 minutos`, el sistema entra automaticamente a la misma rama que `No puedo`

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

- el pedido pasa a `draft_payment`
- el cliente recibe confirmacion de pedido programado con subtotal, domicilio y total final
- si el pedido aplica al por mayor, el bot muestra primero `select_referral_option`
- luego el bot muestra `select_payment_method`

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
- si vence el timer, el cliente puede:
  - dejar mensaje
  - volver al menu

### Relay

El relay se usa para:

- contacto directo con asesor

Comportamiento actual:

- cliente -> asesor: se reenvia con prefijo `[CLIENTE ...xxxx]:`
- asesor -> cliente: se reenvia como texto libre
- el asesor recibe el boton `Finalizar` solo una vez, cuando inicia el relay
- si el relay termina manualmente o por timeout, la conversacion del cliente se resetea a `main_menu`

`relay_kind` identifica el contexto del relay:

- `contact_advisor`

## Timers Activos

Timers de runtime:

- comprobante: `10 minutos`
- espera de asesor para `Hablar con Asesor`: `2 minutos`
- espera de asesor para confirmacion de pedido inmediato: `5 minutos`
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
- `review_checkout`
- `select_payment_method`
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
- `wait_advisor_response`: marca el timeout del pedido inmediato sin fanout en boot
- `wait_advisor_contact`: marca timeout del asesor sin fanout en boot
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
- validar `Dejar mensaje`
- validar relay cliente -> asesor y asesor -> cliente
- validar `Finalizar`

Timers y reinicio:

- comprobar timeout de comprobante
- comprobar que el pedido inmediato pase automaticamente a la rama de `No puedo` despues de `5 minutos`
- comprobar timeout de `Hablar con Asesor`
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
