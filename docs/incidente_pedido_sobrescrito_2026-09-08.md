# Incidente: un pedido de recompra sobrescribió al anterior

**Cliente:** Kall Díaz (`573135248660`), conversación 67, pedido 36.
**Detectado:** 2026-09-08, revisando por qué el chat se quedó sin bot.
**Impacto:** una venta de $104.000 y 20 unidades de inventario fuera del sistema financiero
durante 2 días; el pedido original quedó destruido en la base.

## Qué pasó

1. **2026-08-30** — Kall pide 20 unidades (5 Bonbonbum Whiskey / 5 Smirnoff lulo / 5 Smirnoff
   tamarindo / 5 Uva Vodka), $98.000 + $6.000 domicilio. Se crea el pedido 36, se acepta en
   Pendientes y se registra la venta (factura 28). Entregado el mismo día.
2. **2026-08-31** — durante el QA de Pendientes se cancela el `order_dispatch` del pedido 36
   (la venta ya estaba registrada a mano). El pedido 36 queda con resolución `cancelled`.
3. **2026-09-05** — Kall vuelve a pedir. Como la conversación seguía atada al pedido 36
   (`current_order_id = 36`, `order_confirmed = true`), el bot preguntó "¿modificar ese pedido o
   hacer uno nuevo y separado?". El cliente respondió algo ambiguo ("las últimas cantidades que
   pedí") y el modelo eligió **modificar**.
4. Se llamó `modify_confirmed_order`, que reabre la MISMA orden **conservando sus items**. El
   modelo agregó las 20 unidades nuevas encima de las 20 viejas → carrito de 40 unidades. De ahí
   salió el mensaje al cliente con "Subtotal: $196.000" listando solo 4 líneas que sumaban
   $98.000, y un "te faltan 6 unidades más para completar domicilio gratis" que ninguna
   herramienta devolvió (`units_until_free_delivery` da `None` desde 6 unidades).
5. Al confirmar, `upsert_order_from_context` hizo `update_order` + `replace_order_items` sobre el
   pedido 36: **los items del pedido del 08-30 dejaron de existir**.
6. El pedido modificado **nunca reapareció en Pendientes**: `crm-app` filtra la cola con
   `item.needsHumanAction || item.resolution === null` (`apps/crm/src/server/inbox/queue.ts`) y
   `order_dispatch` está indexado por `order_id`. El pedido 36 ya tenía resolución `cancelled`,
   así que la segunda compra quedó invisible → sin "Aceptar" → sin venta auto-registrada → sin
   consumo FIFO de inventario.
7. Ese mismo día el caso agotó el presupuesto de LLM (30 llamadas/día por teléfono) a las 18:07,
   y el cliente recibió 4 veces el mensaje de límite diario. Eso fue consecuencia del volumen de
   turnos, no del bug — pero fue lo que hizo visible el caso.

## Causa raíz

`current_order_id` sobrevivía indefinidamente al checkout, y la decisión "¿modificar o pedido
nuevo?" quedaba en manos del modelo. Una recompra días después es indistinguible, para el modelo,
de una corrección del pedido en curso.

## Correcciones (v1.24.0)

- **`release_delivered_order_binding` (`src/ai/agent.rs`)** — corre al inicio de cada turno, antes
  de que el modelo vea nada. Si el pedido confirmado ya se entregó, suelta el binding y la
  siguiente compra crea una orden nueva. "Ya se entregó" = inmediato confirmado hace más de
  `IMMEDIATE_ORDER_ACTIVE_HOURS` (6h), o programado cuya fecha/hora ya pasó. Un pedido confirmado
  sin `order_confirmed_at` (estado de una versión anterior) se trata como entregado: crear una
  orden de más es corregible, pisar la anterior no.
- **`order_confirmed_at`** — campo nuevo en `ConversationStateData`/`ConversationContext`, sellado
  en `confirm_order_bookkeeping`. `#[serde(default)]` lo hace compatible con el estado ya
  persistido.
- **`modify_confirmed_order` ahora enumera los items que ya están en el pedido** y advierte
  explícitamente que hay que quitarlos si el cliente dicta su lista completa de nuevo.
- **Una modificación vuelve a Pendientes** — la notificación al asesor de un pedido MODIFICADO
  pasa a `requires_action: true`. Sin eso, un pedido ya aceptado que cambia queda invisible en la
  consola y el asesor entrega lo que decía la versión vieja.
- **`free_delivery_status_line`** — cada `get_order_summary`/`add_order_item` termina con el
  estado del domicilio gratis ya resuelto de forma determinista (no aplica / faltan N / ya
  califica / fuera de Armenia). El modelo tiene prohibido calcular ese número por su cuenta.

## Remediación de datos (2026-09-08)

- Venta de la segunda compra registrada con `recordSale` (mismo camino que `/ventas`):
  **factura 29**, 20 u, $98.000 + $6.000 domicilio, consumo FIFO de 20 u del lote `lote 1 julio`,
  costo $41.533,33, utilidad neta $56.466,67.
- `customers` de Kall corregido a 40 unidades / $208.000 (mostraba solo una de las dos compras,
  porque la ruta de modificación **reemplaza** el snapshot de analytics en vez de sumar).
- Conversación 67 reiniciada (`state_data` vacío, `agent_case_messages` borrado) para que la
  próxima vez que escriba arranque como chat nuevo. `customers`, `customer_addresses` y
  `message_events` se conservaron.

## Lo que NO se recuperó

Los sabores del pedido del 2026-08-30 (5 Bonbonbum Whiskey / 5 Smirnoff lulo / 5 Smirnoff
tamarindo / 5 Uva Vodka) solo sobreviven en el transcript de `message_events`. La fila de
`order_items` del pedido 36 quedó con los sabores del segundo pedido y no se restauró: el pedido
36 hoy representa la compra del 09-05, no la del 08-30.
