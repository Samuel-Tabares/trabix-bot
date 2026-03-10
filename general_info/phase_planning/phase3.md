## Fase 3: Precios, resumen y pago

## Resumen

Partimos de una Fase 2 ya implementada y validada:

- webhook, HMAC, cliente de WhatsApp, PostgreSQL y migraciones funcionando
- máquina de estados persistente integrada en `src/routes/webhook.rs`
- flujo desde menú principal hasta `ShowSummary`
- persistencia de `state` y `state_data` entre mensajes y reinicios
- validaciones de nombre, teléfono, dirección, cantidad, fecha y hora
- programación con lenguaje natural y override local de reloj solo para pruebas

En este punto `ShowSummary` sigue siendo provisional. La Fase 3 lo convierte en un cierre comercial real del pedido:

- cálculo automático de precios
- clasificación detal/mayor
- forma de pago
- comprobante por imagen
- persistencia del pedido en `orders` y `order_items`

La lógica del asesor, el costo de domicilio, la negociación de hora y el relay siguen fuera de esta fase. Fase 3 debe dejar el pedido listo y persistido para ser tomado por Fase 4.

## Cambios de Implementación

- Crear `src/bot/pricing.rs` con:
  - funciones puras de cálculo detal y mayor
  - clasificación del pedido por tipo
  - desglose de subtotales y total estimado
- Crear `src/bot/timers.rs` con infraestructura mínima reutilizable para timers.
  - En Fase 3 solo se usa para `WaitReceipt`
  - timers del asesor siguen pendientes para Fase 4
- Integrar `pricing.rs` y `timers.rs` en:
  - `src/bot/mod.rs`
  - `src/lib.rs`
  - `src/main.rs`
- Extender `src/config.rs` para soportar el texto o datos de pago por transferencia.
  - Preferir una variable tipo `TRANSFER_PAYMENT_TEXT`
  - No hardcodear datos bancarios en el código
- Ampliar `ConversationStateData` y `ConversationContext` solo lo necesario para Fase 3.
  - `current_order_id: Option<i32>` para reanudar pago/persistencia sin ambigüedad
  - `editing_address: bool` para permitir cambio de dirección sin introducir estados extra
- Extender `src/db/queries.rs` para:
  - crear pedido borrador
  - registrar `order_items`
  - actualizar `receipt_media_id`
  - actualizar `status` del pedido durante el flujo de pago
- Reemplazar la implementación provisional de `ShowSummary` en `src/bot/states/checkout.rs`
- Activar en el executor de acciones:
  - `StartTimer`
  - `CancelTimer`
  - `SaveOrder` si se reutiliza como acción
  - `SendText`, `SendButtons`, `SendList`, `ResetConversation` ya existen y deben mantenerse

## Flujo Funcional a Implementar

- Reglas de precio:
  - con licor detal: promo de pares exacta según la spec
  - sin licor detal: $7.000 por unidad
  - mayor: tabla por rangos exacta según la spec
  - la clasificación se hace por separado para `con licor` y `sin licor`
- `ShowSummary`:
  - calcula el pedido completo a partir de `context.items`
  - muestra resumen con:
    - cliente
    - entrega inmediata o programada
    - fecha/hora si aplica
    - items con cantidad, sabor, tipo y subtotal
    - total estimado sin domicilio
    - nota explícita de que el domicilio lo define el asesor después
  - envía una lista interactiva con 4 opciones:
    - `cash_on_delivery`
    - `pay_now`
    - `modify_order`
    - `cancel_order`
- `cash_on_delivery`:
  - guarda `payment_method = cash_on_delivery`
  - crea o actualiza el pedido borrador en DB
  - pasa a `ConfirmAddress`
- `pay_now`:
  - guarda `payment_method = transfer`
  - crea o actualiza el pedido borrador en DB
  - envía instrucciones de transferencia desde config
  - pasa a `WaitReceipt`
- `modify_order`:
  - conserva datos del cliente, programación y tipo de entrega
  - limpia `items`
  - limpia `current_order_id`
  - vuelve a `SelectType`
- `cancel_order`:
  - resetea conversación
  - limpia cualquier borrador de pedido que no deba continuar
  - vuelve a `MainMenu`
- `WaitReceipt`:
  - solo acepta imagen como comprobante válido
  - si llega texto o botón inesperado, corrige y repite la instrucción
  - al entrar, inicia timer de 10 minutos
  - si llega imagen:
    - cancela timer
    - guarda `receipt_media_id`
    - actualiza el pedido borrador
    - pasa a `ConfirmAddress`
  - si expira timer:
    - informa timeout
    - ofrece `change_payment_method` o `cancel_order`
- `ConfirmAddress`:
  - muestra la dirección actual y botones:
    - `confirm_address`
    - `change_address`
  - `change_address` activa `editing_address = true`
  - el siguiente texto válido reemplaza la dirección en contexto y en DB
  - vuelve a pedir confirmación de dirección
  - `confirm_address`:
    - persiste el pedido final de Fase 3 en `orders` y `order_items`
    - deja el pedido en estado `pending_advisor`
    - transiciona a `WaitAdvisorResponse`
    - en Fase 3 solo envía un mensaje textual de cierre tipo:
      - "Tu pedido quedó registrado y será confirmado por un asesor."
- Ruta mayor en Fase 3:
  - sí se calcula y se persiste
  - no entra todavía a relay
  - si un pedido incluye mayor, igualmente debe terminar en `pending_advisor`

## Persistencia y Estados

- `conversations.state` sigue persistiéndose como string `snake_case`
- `conversations.state_data` debe guardar al menos:
  - `items`
  - `delivery_type`
  - `scheduled_date`
  - `scheduled_time`
  - `payment_method`
  - `receipt_media_id`
  - `current_order_id`
  - `editing_address`
- `orders` se empieza a usar activamente en Fase 3
- `order_items` se crean con el cálculo resultante del resumen, no solo copiando cantidad sin precio
- Estados de pedido sugeridos para esta fase:
  - `draft_payment`
  - `waiting_receipt`
  - `pending_advisor`
  - `cancelled`

## Decisiones y Defaults

- El total mostrado en Fase 3 sigue siendo sin domicilio.
- El domicilio lo define el asesor en Fase 4.
- No implementar todavía:
  - envío real al asesor
  - costo de domicilio
  - negociación de hora con asesor
  - relay
  - timers del asesor
- El timer de Fase 3 es solo el de comprobante.
- `TRANSFER_PAYMENT_TEXT` debe venir de variables de entorno para poder cambiar datos bancarios sin tocar código.
- Entradas inesperadas deben responder con corrección y repetir la instrucción actual, igual que en Fase 2.
- Si el servidor se reinicia con un comprobante pendiente, el timer debe recrearse.

## Pruebas

- Unit tests para:
  - promo con licor detal en cantidades 1, 2, 3, 4 y 5
  - rangos de mayor en 20, 49, 50, 99 y 100
  - pedido mixto detal/mayor
  - serialización de `current_order_id` y `editing_address`
  - transiciones puras de:
    - `ShowSummary`
    - `WaitReceipt`
    - `ConfirmAddress`
- Integración local para:
  - persistencia de órdenes y `order_items` en PostgreSQL
  - recreación del timer de `WaitReceipt` tras reinicio
  - actualización de `receipt_media_id`
- Validación manual end-to-end:
  - pedido detal con `Contra Entrega`
  - pedido detal con `Pago Ahora` + envío de imagen
  - pedido mixto con clasificación detal/mayor correcta
  - cambio de dirección antes del cierre
  - timeout de comprobante y cambio de forma de pago

## Criterio de Cierre de Fase 3

- `ShowSummary` ya no es provisional.
- Los precios se calculan correctamente según la spec.
- El cliente puede elegir `Contra Entrega` o `Pago Ahora`.
- `Pago Ahora` acepta imagen como comprobante y maneja timeout.
- El pedido y sus items se guardan en PostgreSQL con total estimado y método de pago.
- El flujo termina con pedido persistido y conversación en `WaitAdvisorResponse` o equivalente de handoff, sin implementar todavía la lógica real del asesor.
- Los timers de comprobante sobreviven reinicios del servidor.

## Supuestos

- Seguimos usando el número tester de Meta para Fase 3.
- El texto de transferencia se definirá por variable de entorno.
- La validación manual local puede seguir usando túnel temporal si Railway continúa inestable.
- El comportamiento ya introducido en Fase 2 para fecha/hora natural y `FORCE_BOGOTA_NOW` se mantiene tal como está.
- Después de este plan, el siguiente punto de integración principal sigue siendo `src/routes/webhook.rs`, con trabajo adicional en `src/bot/states/checkout.rs`, `src/bot/pricing.rs`, `src/bot/timers.rs` y `src/db/queries.rs`.
