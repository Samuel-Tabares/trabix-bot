# Referencia Operativa Actual

## Resumen

Este documento reemplaza los antiguos documentos de `general_info/phase_planning/`.
Su objetivo es describir el funcionamiento real y actual del bot en produccion, con el flujo vigente, la persistencia real, los timers activos, las dependencias operativas y la validacion practica del servicio.

Este archivo debe mantenerse alineado con:

- `CLAUDE.md`
- `general_info/complex_diagram.md`
- `general_info/simple_diagram.md`
- la implementacion vigente en `src/`
- `LICENSE`

Licenciamiento actual del repositorio:

- el repositorio es propietario y se distribuye bajo `All Rights Reserved`
- la visibilidad publica del codigo no concede permiso para copiarlo, modificarlo, redistribuirlo o venderlo
- solo se permite ver el codigo para evaluacion personal no comercial, segun `LICENSE`

## Arquitectura Actual

El proyecto es un servicio Rust production-only. El unico runtime recibe webhooks de Meta Cloud
API, valida la firma HMAC, clasifica los mensajes entre cliente y asesor, ejecuta una maquina de
estados persistente y responde por WhatsApp usando texto, botones, listas e imagenes. El simulador
local fue eliminado en v1.8.0.

Componentes principales:

- `src/routes/`
  - `verify.rs`: verificacion de `GET /webhook`
  - `webhook.rs`: recepcion del webhook productivo y normalizacion de inputs
  - `legal.rs`: paginas publicas `/privacy-policy` y `/terms-of-service`
  - `internal.rs`: `POST /internal/advisor/send`, salida de WhatsApp para `crm-app` autenticada con
    el header `X-Internal-Token` (`INTERNAL_API_TOKEN`; sin la variable el endpoint responde 503).
    No es un camino de webhook: no cambia el estado de la conversacion ni pausa timers, solo toma el
    lock del caso, envia texto y traza en `message_events` (`channel='client'`, `actor='advisor'`,
    `payload.source='crm-app'`). Contrato: `docs/internal_advisor_send.md`. Desde Fase 8, `mod.rs`
    sirve estas rutas (y las de `referral-codes`) en un `internal_router()` separado, en su propio
    listener (`INTERNAL_PORT`, default 8081) sin dominio publico en Railway — `public_router()`
    (`/webhook`, `/privacy-policy`, `/terms-of-service`) queda solo en `PORT`. Antes ambos grupos
    compartian un unico listener y `/internal/*` era alcanzable desde internet (protegido solo por
    el token); ver `../main.rs` y `../general_info/runbook.md`.
- `src/engine.rs`
  - procesamiento compartido de cliente/asesor
  - ejecucion compartida de acciones para webhook y timers
- `src/whatsapp/`
  - cliente de Meta Cloud API (es el `AppState.transport`)
  - builders de botones y listas
  - tipos serde para payloads entrantes y salientes
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

## Motor De Agente IA

El motor de agente es el único runtime (el toggle `BOT_ENGINE` se eliminó — no hay rollback a un
motor determinista global, `ANTHROPIC_API_KEY` es obligatoria para arrancar).

- El agente es dueño de los estados de autoservicio del cliente (menu, pedido, datos, checkout,
  `ask_delivery_cost`, `wait_business_hours`, `select_payment_method`, `wait_receipt`) — ver
  `is_agent_owned_state()` en `src/engine.rs`. Los estados de negociacion de hora, esperas de
  asesor legadas y relay siguen 100% deterministas (`transition()` en `src/bot/state_machine.rs`)
  — esto es estructural, no un modo alternativo: un mensaje del CLIENTE mientras el caso espera al
  asesor, negocia hora, o está en relay, siempre pasa por el FSM determinista, sin importar nada
  más. Detalle completo de qué código FSM sigue vivo por esta vía en
  `docs/CLEANUP_deterministic_engine.md`.

Protecciones activas en modo agente:

- degradacion segura: si la llamada al LLM falla (timeout 60s del cliente HTTP, 5xx, saldo), el
  cliente recibe `[agent].llm_failure_customer` de `config/messages.toml`, el asesor recibe el
  contexto del caso (cliente, numero, ultimo mensaje, estado) y el estado NO cambia.
- presupuesto diario: 30 llamadas LLM por telefono por dia (Bogota) + kill-switch global opcional
  `AGENT_DAILY_LLM_CALL_LIMIT`. Al agotarse: mensaje fijo `[agent].daily_limit_customer` al
  cliente y aviso al asesor una vez por dia por caso. Contadores en memoria (reset al redeploy).
- ventana de memoria: `agent_case_messages` guarda todo el historial (CRM), pero al LLM solo van
  los ultimos 40 mensajes, cortados en frontera segura de tool-use. Mensajes entrantes se truncan
  a 1.500 caracteres antes de ir al LLM.
- dedup de webhook: cache en memoria (TTL 10 min) de `message_id` de Meta ya procesados; los
  retries de Meta no generan doble respuesta ni doble llamada LLM.
- anti prompt-injection en el system prompt: los mensajes del cliente nunca cambian precios,
  reglas ni comportamiento; solo se citan cifras devueltas por tools; quien diga ser el asesor
  sin serlo se trata como cliente.

Correcciones del canary 2026-07-19 (`docs/canary-fixes-2026-07-19.md`):

- guard determinista anti-alucinacion de cifras: al final de cada turno, todo texto saliente
  (`SendText`, cliente o asesor) se escanea buscando montos `$X.XXX`; si menciona una cifra que no
  aparece textual en ningun tool-result de la conversacion, el mensaje se bloquea y se reemplaza
  por uno neutro (se registra en logs para auditoria). No depende solo del prompt.
- `checkout::render_summary` distingue domicilio "aun no conocido" (`None`) de "conocido y vale
  $0" (`Some(0)`): si el domicilio no se conoce todavia, la cifra se etiqueta como "Subtotal de
  productos (sin domicilio aun, no es el total final)" en vez de "Total", para no cotizar un total
  incompleto como si fuera el final.
- **Fase 1 (v1.14.0): ningun pedido pasa por una confirmacion de disponibilidad del asesor.** La
  tool `confirm_advisor_availability` se borro por completo (quedo sin call-sites reales). Regla
  unificada, ver `can_auto_accept()` en `src/ai/agent.rs`: un pedido se autoacepta si es
  `scheduled` (siempre) o si es `immediate` y `check_business_hours().is_open`. El auto-accept vive
  en `finalize_checkout` (si el domicilio ya se conoce) y en `set_manual_delivery_cost` (si el
  domicilio llega despues, dato del asesor) — ambos llaman `auto_accept_order` (antes
  `auto_accept_scheduled_order`, generalizada), que calcula el total, deja el pedido en
  `draft_payment` y salta directo a `select_payment_method`, avisandole al asesor de forma
  informativa (nunca le pregunta disponibilidad).
  - Pedido `immediate` **fuera de horario**: ya no se rechaza ni se fuerza a programar
    (`checkout_precondition_error`/`set_delivery_immediate` ya no bloquean esto). Queda guardado
    (`pending_advisor`) en el estado nuevo `ConversationState::WaitBusinessHours` — deliberadamente
    distinto de `ask_delivery_cost` porque ese hereda el timeout de 10-30 min por inactividad
    (`advisor_timeout_kind` → `HardReset`) que habria reseteado el pedido antes de que abrieramos.
    Se resuelve solo cuando el sweep de 60s (`sweep_expired_timers`) detecta que
    `check_business_hours().is_open` volvio a ser cierto (nuevo `TimerType::BusinessHoursReopen`,
    `timers::expire_business_hours_timer_with_source`): si el domicilio ya se conocia, autoacepta
    y avisa al cliente; si no, pasa a pedir el costo con el timer normal de 10 min (ya es horario
    real). No hay duracion que trackear ni timer en vivo armado con `StartTimer` — el sweep ya
    corria cada 60s para los demas timers.
  - Fuera de alcance a proposito: destinos desconocidos (envio nacional). Siempre son `scheduled`,
    ya auto-aceptan al conocerse el costo via `set_manual_delivery_cost` y ya quedan visibles como
    `needs_human` en `crm-app` sin cambios — es el unico paso bloqueante que sigue en pie.
- el atajo deterministico de comprobante (`try_handle_receipt_shortcut`, resend de la imagen al
  asesor) ya no depende solo de `current_state == wait_receipt`: tambien dispara si
  `payment_method == transfer` y todavia no hay `receipt_media_id`, para no perder el reenvio del
  comprobante ante un desface de estado.
- catalogo: Maracumango, Manzana verde, Bonbonbum y Blueberry existen como productos DISTINTOS con
  y sin licor bajo un mismo nombre base (ids ya separados en `config/messages.toml`, eso ya
  funcionaba). Lo que faltaba era desambiguacion: `add_order_item` ahora exige tambien
  `customer_wording` (la frase literal del cliente) y rechaza el intento si el nombre es ambiguo
  (ej. "manzana" a secas) y esa frase no trae ninguna palabra que distinga la variante (ron,
  tequila, vodka, whiskey, champaña, "con/sin licor") — tabla en
  `tools::check_flavor_disambiguation`. Sabores que solo existen en una variante (Uva Vodka,
  Smirnoff de lulo) no requieren ninguna palabra extra.

Correcciones del canary 2026-07-20 (resto del backlog de `docs/canary-fixes-2026-07-19.md`):

- CICLO DE VIDA DEL PEDIDO CONFIRMADO (hallazgo A): al confirmar (efectivo o comprobante) el motor
  agente YA NO emite `ResetConversation` — el contexto persiste con `current_order_id` intacto y un
  flag `order_confirmed=true`. Un pedido confirmado no se puede re-confirmar: `finalize_checkout`
  lo rechaza. Para cambiarlo, el LLM llama `modify_confirmed_order` (reabre la MISMA orden → el
  checkout hace UPDATE, no crea otra); para un pedido aparte llama `start_new_order` (suelta el
  binding). Analytics es por DELTA: `confirmed_order_snapshot` guarda lo ya acumulado y la
  re-confirmacion suma solo la diferencia sin re-contar `times_used` (`referral_times_used_inc` en
  `UpdateCustomerAndAnalytics`). El motor determinista sigue reseteando y nunca setea snapshot, asi
  que su comportamiento no cambia.
- CODIGO DE REFERIDO OBLIGATORIO EN MAYORISTA (item 9): `finalize_checkout` bloquea la confirmacion
  de un pedido con bucket mayorista hasta que el tema del codigo este resuelto — se aplica un codigo
  valido (`apply_referral_code`) o el cliente dice que no tiene (nuevo tool `skip_referral_code`);
  flag `referral_prompt_resolved`. En retail el codigo no aplica (pricing ya lo rechaza), no se
  pregunta.
- HORARIO INYECTADO (item 1): el bloque "ESTADO ACTUAL DEL CASO" del system prompt incluye la hora
  y dia actuales de Bogota y si esta ABIERTO/CERRADO para entrega inmediata, en cada turno. El LLM
  no responde de memoria ni depende de llamar `check_business_hours`.
- RECAP OBLIGATORIA (item 7): antes de confirmar cualquier pedido (o de mandar datos de
  transferencia) el prompt exige recapitular productos+variante+cantidad, fecha/hora absolutas,
  direccion y total con domicilio, y esperar OK explicito. Refuerzo de prompt.
- CERO BOTONES EN MODO AGENTE (item 3): el primer contacto responde un saludo fijo `[agent].welcome`
  SIN llamar al LLM (flag `has_greeted`); el resto es texto LLM. Los timers que emitian botones
  (receipt/contact/advisor timeout) mandan solo texto plano en modo agente — los estados ya son
  agent-owned, asi que la respuesta del cliente la interpreta el LLM. El motor determinista conserva
  sus botones.
- FORMATO WhatsApp (item 6): `normalize_whatsapp_markdown` colapsa `**x**`→`*x*` y `__x__`→`_x_` en
  cada `SendText` saliente del agente.
- DATOS META VS PERSONALIZADOS (hallazgo C): `meta_customer_name`/`meta_customer_phone` (base
  inmutable de Meta) se separan del nombre/celular personalizado; `set_customer_field` guarda los
  personalizados sin validacion; el paquete al asesor muestra ambos ("personalizado (Meta: real)").
  Sin migracion: la tabla `customers` ya tenia columnas meta/manual.

Auditoria de alcanzabilidad del relay (2026-07-15): en modo agente NO existe camino alcanzable a
`relay_mode` ni `wait_advisor_contact` para conversaciones creadas bajo el agente:

- `relay_mode` solo se entra desde `wait_advisor_mayor` (boton Tomar) o `wait_advisor_contact`
  (boton Atender), y esos dos estados solo se alcanzan desde handlers deterministas de estados
  que en modo agente posee el agente (`main_menu`, `out_of_hours`, `review_checkout`).
- el timer de asesor que arranca `finalize_checkout` expira en `ask_delivery_cost`, cuyo timeout
  es `HardReset` (pedido a `manual_followup` + reset a `main_menu`), nunca relay ni
  `negotiate_hour` (la rama `AutoCannot` -> `negotiate_hour` solo aplica a
  `wait_advisor_response`, inalcanzable en modo agente).
- unica excepcion intencional: conversaciones que caen en un estado no agent-owned (esperas de
  asesor, negociacion de hora, relay — ver `is_agent_owned_state()`) siguen su flujo determinista
  completo mientras esten ahi, incluido relay.

## Ruteo Real Del Webhook

Flujo base:

1. Meta llama `POST /webhook`.
2. El bot valida `X-Hub-Signature-256`.
3. Si la firma es valida, responde `200` de inmediato y procesa de forma asincrona.
4. Si el payload no trae mensajes entrantes, el bot solo registra el evento y no ejecuta flujo conversacional.
5. Si el `from` coincide con `ADVISOR_PHONE`, el mensaje entra siempre al flujo de asesor.
6. Cualquier otro numero entra al flujo de cliente.

Excepcion keepalive: si el asesor envia exactamente `✅` (solo el emoji, espacios ignorados),
el bot lo ignora en silencio — sin respuesta, sin ruteo, sin escritura en BD
(`is_window_keepalive_ping` en `src/engine.rs`). Es el ping diario del asesor para mantener
abierta la ventana de servicio de 24h de WhatsApp: si nadie escribe al numero del asesor en 24h,
Meta trata los mensajes salientes hacia el como plantilla/publicidad y los rechaza.

Comportamiento actual relevante:

- El log `received whatsapp message` se emite antes de `mark_as_read` y antes de consultar la base de datos, para distinguir recepcion real de fallos internos.
- Los payloads Meta de clientes externos pueden incluir `contacts[].profile.name`, mensajes interactivos y `context` ausente o sin `id`; esos casos deben parsear sin romper ni redirigir el mensaje.
- `mark_as_read` es best-effort. Si Meta rechaza ese request, el bot solo registra warning y sigue procesando.
- Los logs del runtime priorizan visibilidad operativa con telefono enmascarado, previews cortos de texto, transiciones de estado, resumen de respuestas salientes y eventos de timers. Los callbacks de estado `sent/delivered/read` de Meta deben quedar fuera del ruido normal de `INFO` y verse en `DEBUG` cuando haga falta.
- El callback productivo exacto es `/webhook`.
- Para trafico publico real, la app de Meta debe estar en `Live` y el WABA debe estar suscrito a la app activa.

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

### Direcciones Guardadas (Recompra, v1.16.0)

Además de `customers.delivery_address_last` (texto libre, siempre el último, se sigue
sobreescribiendo en cada turno sin cambios), el motor agente puede guardar hasta 4 direcciones
reales por cliente en `customer_addresses` (migración `015`), cada una con su zona ya resuelta
(`zone_kind`/`zone_value`, reconstruible con `bot::delivery_zone::ArmeniaZone::from_storage_key`/
`lookup_nearby_town`) y un `last_delivery_cost_cop` de referencia (informativo — el costo real de
un checkout SIEMPRE sale de una tool call en vivo, nunca de este snapshot).

- `engine.rs` prefetchea la lista (`queries::list_customer_addresses`) antes de cada turno de
  agente y la pasa como parámetro — no se persiste en `ConversationContext`/`state_data`.
- Tool `list_saved_addresses`: formatea esa lista, sin tocar la DB.
- Tool `select_saved_address(address_id)`: reutiliza una guardada, deja `pending_zone_kind/
  value/label` en el contexto (sí persistidos en `state_data`, para sobrevivir hasta la
  confirmación) y dispara `BotAction::TouchCustomerAddress` (bump de `last_used_at`). El modelo
  igual debe llamar `set_delivery_zone_armenia`/`set_delivery_nearby_town` después para fijar el
  costo real (si `zone_kind == "national"`, en vez de eso vuelve a pedir el costo al asesor —
  ver más abajo, la tarifa de una transportadora puede haber cambiado).

### Envío Nacional (v1.19.0)

Tercera forma de entrega, junto a Armenia (moto propia, zonas) y los 13 municipios (moto propia,
tarifa fija en `bot::delivery_zone`): cualquier destino fuera de esas dos listas se despacha por
transportadora nacional, producto **descongelado** (el cliente lo congela al recibirlo — promesa
distinta a Armenia/municipios, donde llega listo para consumir).

- `bot::delivery_zone::MIN_UNITS_NATIONAL = 20`, sin excepción.
- Tool `set_delivery_national(city)`: valida el mínimo y fija `pending_zone_kind = "national"`,
  `pending_zone_value = None`, `pending_zone_label = "Envío nacional (transportadora) — {city}"`.
  No calcula tarifa — el costo sigue el camino ya existente de domicilio manual
  (`message_advisor` + `set_manual_delivery_cost`, mismo mecanismo que un municipio desconocido),
  así que `finalize_checkout`, el estado `ask_delivery_cost` y la recuperación por timer de
  horario (`timers::expire_business_hours_timer_with_source`) se reusan sin cambios.
- El resultado de la tool trae el aviso obligatorio de "llega descongelado" en el texto que el
  modelo debe repetirle al cliente — reforzado también como regla dura en `SYSTEM_PROMPT`, para
  que no dependa solo de que el modelo la recuerde.
- Escala únicamente por `needs_human`/consola (`message_advisor` ya respeta
  `ADVISOR_WHATSAPP_ENABLED=false`), nunca por WhatsApp directo al asesor.
- Al confirmar el pedido (`confirm_order_bookkeeping`, llamado desde el flujo de transferencia y
  el de contraentrega), si hay `delivery_address` + zona pendiente resueltos se dispara
  `BotAction::UpsertCustomerAddress`: `queries::upsert_customer_address` actualiza la dirección si
  ya existía (mismo `address_key` normalizado) o inserta una nueva, descartando la de `created_at`
  más antiguo si el cliente ya tenía las 4.

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
- valida el codigo contra la tabla `referral_codes` (Fase 6, `migrations/016`), cacheada en memoria
  (`src/referrals.rs`, refresco en background cada 30s + refresco inmediato tras cada escritura)
- los codigos guardados deben cumplir estas reglas:
  - solo minusculas
  - sin espacios
  - maximo `15` caracteres
- el boost es una ventana temporal por codigo (`boost_until`), no una lista estatica: expira solo a
  los 7 dias de activarse, no se acumula si se reactiva antes de expirar
- gestion de codigos (crear, activar/desactivar, activar boost) vive en `crm-app` → sección
  "Embajadores", que llama a `POST/PATCH /internal/referral-codes*` (mismo `X-Internal-Token` que
  `/internal/advisor/send`) — el bot sigue siendo el unico escritor de la tabla

Si el codigo es invalido:

- el bot sigue en `wait_referral_code`
- muestra botones `Reintentar código` y `Seguir sin código`

Si el codigo es valido:

- el descuento se aplica solo sobre los buckets ya calculados como `mayor`
- cada bucket elegible calcula su tier de forma independiente:
  - `20-49`: cliente `10%`, embajador `15%`
  - `50-99`: cliente `12%`, embajador `18%`
  - `100+`: cliente `15%`, embajador `20%`
- si el codigo esta en `boost_codes`, la comision del embajador suma `5` puntos porcentuales sin cambiar el descuento del cliente
- el descuento del cliente siempre se redondea hacia arriba al siguiente centenar:
  - ejemplo: `$4.510` pasa a `$4.600`
  - si ya cae exacto en centenar, se mantiene igual
- el domicilio no participa en el descuento ni en la comision
- el bot recalcula:
  - `referral_discount_total`
  - `ambassador_commission_total`
  - `total_final = subtotal_con_descuento + delivery_cost`
- el cliente recibe confirmacion del codigo aplicado
- el cliente vuelve a ver el resumen listo para pago con subtotal, descuento referido, domicilio y total final
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
- si el asesor no digita el domicilio en `ask_delivery_cost`, el cutoff de pedido programado es `23 horas`

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
- si el asesor no digita el domicilio en `ask_delivery_cost`, el cutoff de pedido inmediato sigue siendo `30 minutos`

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

Timers de runtime (consolidados en FASE 5):

- comprobante: `10 minutos`
- espera de asesor (todos los estados de espera de asesor, incluidos
  `wait_advisor_response`, `wait_advisor_contact` y los waits detallados como
  `ask_delivery_cost`): `5 minutos` unificados
- relay: `30 minutos` (solo aplica al flujo determinista legado; el modo
  agente no usa relay)
- inactividad generica del cliente:
  - recordatorio a los `2 minutos`, una sola vez
  - no hay reinicio automatico por inactividad; el bot queda esperando input

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
- despues del recordatorio no hay reset: la conversacion queda esperando al
  cliente indefinidamente y no se dispara nada mas hasta que escriba de nuevo

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
- `ask_delivery_cost`, `negotiate_hour`, `wait_advisor_hour_decision`, `wait_advisor_confirm_hour`: puede resetear y mover orden a `manual_followup` sin enviar mensajes en boot; todos usan el timeout unificado de `5 minutos`
- `relay_mode`: cierra silenciosamente si ya estaba vencido
- inactividad generica:
  - si el recordatorio ya debia salir, lo marca como consumido en silencio
  - no hay reset por inactividad en boot ni en runtime

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
- `human_takeover_until` (migración 014, Fase 2)

`state` se persiste como string `snake_case`.

### Toma De Control Humana (Fase 2)

`human_takeover_until` es un timestamp opcional, no un estado (`state` no cambia). Solo lo escribe
`POST /internal/advisor/send` (`crm-app`), cada vez que un asesor manda texto libre al cliente desde
la consola — es la señal de "esto lo está atendiendo un humano", sin botón ni flag manual. Ventana
deslizante: cada envío nuevo la reemplaza a `now + ADVISOR_TAKEOVER_HOURS` (env, default `6`, no
acumula). `POST /internal/advisor/reply` deliberadamente NO la toca (existe para que el bot siga el
checkout automático después de que el asesor destraba una pregunta puntual, ver
`docs/internal_advisor_send.md`).

Mientras `human_takeover_until` está en el futuro:

- `engine::process_customer_input` no llama al agente para ese cliente (el mensaje entrante sigue
  quedando en `message_events`, solo que el bot no lo procesa ni le agenda timers nuevos).
- Los 4 `expire_*_with_source` de `bot::timers` (`advisor`, `relay`, `conversation_abandon`,
  `business_hours`) y la reconciliación de timers vencidos al boot se vuelven no-op.

`POST /internal/advisor/release` la limpia antes de tiempo (botón "Devolver al bot" en `crm-app`).

### `state_data`

Campos mas importantes hoy:

- `items`
- `delivery_type`
- `customer_review_scope`
- `scheduled_date`
- `scheduled_time`
- `payment_method`
- `referral_code`
- `referral_has_boost`
- `referral_discount_total`
- `ambassador_commission_total`
- `receipt_media_id`
- `receipt_timer_started_at`
- `advisor_target_phone`
- `advisor_reply_threads`
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

### Tabla `message_events` (migracion 010)

Traza append-only de cada mensaje que pasa por el bot, para que `crm-web/` pueda reproducir la
conversacion completa. Campos: `case_phone` (el cliente del caso — los mensajes con el asesor
tambien se agrupan bajo el telefono del cliente), `channel` (`client` = carril cliente↔bot,
`advisor` = carril interno bot↔asesor), `actor` (`client` / `bot` / `advisor`), `content_type`
(`text`, `buttons`, `list`, `image`, `button_reply`, `list_reply`), `body`, `payload` (JSONB con
botones/listas/media_id), `wa_message_id`, `created_at`.

Se escribe best-effort en las costuras compartidas (`execute_actions`, `send_timer_actions`,
entradas de cliente y asesor, saludo del agente, degradacion por falla del LLM): un fallo de
logging solo genera warning y nunca bloquea la entrega. Aplica a ambos motores. El ping `✅` del
asesor no se registra. La tabla solo captura hacia adelante (no hay backfill de conversaciones
previas a la migracion).

## Configuracion Y Operacion

Variables y datos operativos clave:

- `DATABASE_URL`
- `TEST_DATABASE_URL`
- `WHATSAPP_TOKEN`
- `WHATSAPP_PHONE_ID`
- `WHATSAPP_VERIFY_TOKEN`
- `WHATSAPP_APP_SECRET`
- `ADVISOR_PHONE`
- `MENU_IMAGE_MEDIA_ID`
- `ANTHROPIC_API_KEY` (obligatoria — sin ella el bot no arranca)
- `AGENT_DAILY_LLM_CALL_LIMIT` (opcional, kill-switch global de llamadas LLM por dia)
- `WABA_ID`, `META_CAPI_DATASET_ID`, `META_CAPI_ACCESS_TOKEN` (opcionales; sin las tres, el
  reporte de compras a la Conversions API de Meta es un no-op silencioso — ver
  `docs/PENDIENTE_capi_meta.md`)
- `ADVISOR_TAKEOVER_HOURS` (opcional, default `6` — cuántas horas dura la pausa del bot tras un
  `sendText` desde `crm-app`; ver "Toma De Control Humana" arriba)

Notas actuales:

- los mensajes del cliente viven en `config/messages.toml`
- `TRANSFER_PAYMENT_TEXT` queda como fallback legado si `config/messages.toml` no define el texto de transferencia
- las sesiones PostgreSQL del bot usan `America/Bogota`; la hora efectiva es siempre `Utc::now()` en `America/Bogota` (ya no hay override de reloj)
- `WHATSAPP_TEST_RECIPIENT` sirve para smoke tests live, no define el numero productivo escuchado por el bot

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
- comprobar recordatorio unico de inactividad del cliente (sin reset posterior)
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
