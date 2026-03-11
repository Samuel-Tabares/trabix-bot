## Validacion de Fase 3

### Resumen

Esta validacion se cierra contra el plan aprobado para Fase 3:

- `ShowSummary` ya calcula precios reales y deja de ser provisional.
- El cliente puede elegir `Contra Entrega` o `Pago Ahora`.
- `WaitReceipt` acepta imagen, maneja timeout y recrea timer tras reinicio.
- El pedido se persiste en `orders` y `order_items`.
- El flujo cierra en `pending_advisor`, sin implementar todavia relay ni logica real del asesor.

La evidencia se divide en 4 capas: local automatizada, DB live, WhatsApp live manual y reinicio controlado.

### Precondiciones

- `.env` con `DATABASE_URL`, `TEST_DATABASE_URL`, `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_ID`, `WHATSAPP_VERIFY_TOKEN`, `WHATSAPP_APP_SECRET`, `ADVISOR_PHONE`, `TRANSFER_PAYMENT_TEXT` y `MENU_IMAGE_MEDIA_ID`.
- `ADVISOR_PHONE` debe ser distinto a `WHATSAPP_TEST_RECIPIENT`.
- El webhook de Meta debe apuntar al servicio activo.
- Si se quiere probar fuera de horario o escenarios de agenda, se puede usar localmente `FORCE_BOGOTA_NOW=2026-03-10 23:30`.
- No usar `FORCE_BOGOTA_NOW` en Railway o produccion.
- Si todavia no tienes `MENU_IMAGE_MEDIA_ID`, sube la imagen unica del menu con:

```bash
cargo run --bin upload_media -- /ruta/local/menu.jpg
```

  Luego pega el `media_id` devuelto en `MENU_IMAGE_MEDIA_ID`.

### Comandos de validacion

1. Suite local:

```bash
cargo test
```

2. Smoke DB live:

```bash
cargo test --test live_db -- --ignored --test-threads=1
```

3. Smoke WhatsApp live de transporte:

```bash
cargo test --test live_whatsapp -- --ignored --test-threads=1
```

4. Servicio local para validacion manual end-to-end:

```bash
PORT=8080 cargo run
```

### Matriz de aceptacion

- 2 granizados con licor detal muestran subtotal de `$12.000`.
- 3 granizados con licor detal muestran subtotal de `$20.000`.
- Pedido mixto clasifica por separado `con licor` y `sin licor`.
- `ShowSummary` muestra cliente, entrega, items, total estimado y nota de domicilio pendiente.
- `cash_on_delivery` crea o actualiza borrador y avanza a `ConfirmAddress`.
- `pay_now` crea o actualiza borrador, envia `TRANSFER_PAYMENT_TEXT` y entra a `WaitReceipt`.
- `WaitReceipt` solo acepta imagen como comprobante valido.
- Si llega texto inesperado en `WaitReceipt`, el bot corrige y repite la instruccion.
- Si expira el timer de comprobante, el bot ofrece `change_payment_method` o `cancel_order`.
- `change_payment_method` vuelve a `ShowSummary` sin perder los items.
- `modify_order` conserva datos del cliente y agenda, limpia items y vuelve a `SelectType`.
- `ConfirmAddress` permite `confirm_address` y `change_address`.
- `change_address` actualiza direccion en contexto y en columnas de `conversations`.
- `confirm_address` deja el pedido en `pending_advisor`.
- Reiniciar el servidor con una conversacion en `wait_receipt` recrea el timer.

### Evidencia automatizada

- `src/bot/pricing.rs`
  - cubre promo con licor en cantidades 1, 2, 3, 4 y 5
  - cubre rangos mayor en 20, 49, 50, 99 y 100
  - cubre pedido mixto detal/mayor
- `src/bot/states/checkout.rs`
  - cubre transiciones de `ShowSummary`, `WaitReceipt` y `ConfirmAddress`
- `src/bot/state_machine.rs`
  - cubre serializacion y roundtrip de `current_order_id` y `editing_address`
- `tests/live_db.rs`
  - valida persistencia de `orders` y `order_items`
  - valida actualizacion de `receipt_media_id`
  - valida recreacion de timer en `wait_receipt`

### Validacion manual de WhatsApp

Checklist manual:

1. Enviar `hola` y entrar por `Hacer Pedido`.
2. Completar un pedido detal con `2` con licor y finalizar.
3. Verificar en `ShowSummary` que el total refleje `$12.000`.
4. Elegir `Contra Entrega`.
5. Confirmar direccion y verificar mensaje final de handoff.
6. Consultar PostgreSQL y confirmar:
   - `orders.status = pending_advisor`
   - `orders.payment_method = cash_on_delivery`
   - `order_items` persistidos con subtotales correctos
7. Repetir flujo con `Pago Ahora`.
8. Verificar que el bot envie el texto de `TRANSFER_PAYMENT_TEXT`.
9. Enviar una imagen como comprobante.
10. Confirmar direccion y verificar:
   - `orders.payment_method = transfer`
   - `orders.receipt_media_id` no es null
   - `orders.status = pending_advisor`
11. Probar pedido mixto: por ejemplo `3` con licor + `25` sin licor.
12. Verificar que el resumen muestre ambos tramos con total correcto.
13. Probar `Cambiar direccion` antes de confirmar.
14. Verificar en DB que `conversations.delivery_address` se actualiza.
15. Probar timeout de comprobante:
   - entrar por `Pago Ahora`
   - no enviar imagen durante 10 minutos
   - verificar que llegue el mensaje de timeout con opciones
16. Elegir `Cambiar pago` y verificar retorno a `ShowSummary`.
17. Elegir `Cancelar` y verificar reset a `MainMenu`.

### Validacion de reinicio

1. Iniciar pedido y llegar a `WaitReceipt`.
2. Confirmar en DB que `conversations.state = wait_receipt`.
3. Reiniciar el servicio antes de enviar el comprobante.
4. Verificar en logs de arranque que el servicio sube sin error.
5. Enviar la imagen dentro del tiempo restante y confirmar que el flujo continua a `ConfirmAddress`.
6. Repetir el escenario dejando vencer el tiempo restante y confirmar que dispara el timeout.

### Evidencia esperada

- Capturas o screen recording de WhatsApp para:
  - `ShowSummary`
  - `Pago Ahora`
  - envio de comprobante
  - timeout de comprobante
  - cambio de direccion
- Query o captura de DB para:
  - fila en `orders`
  - filas en `order_items`
  - `conversations.state`
  - `conversations.state_data`
- Logs del servicio para:
  - recreacion del timer al arrancar
  - expiracion del timer o cancelacion por imagen

### Criterio de cierre

- `cargo test` en verde.
- Smoke DB live en verde.
- Smoke de transporte WhatsApp en verde.
- Checklist manual completada con evidencia para:
  - detal contra entrega
  - detal con transferencia e imagen
  - pedido mixto
  - cambio de direccion
  - timeout de comprobante y cambio de pago
- Confirmacion en DB de `pending_advisor`, `receipt_media_id` y `order_items` persistidos.
