## Validacion de Fase 4

### Resumen

Esta validacion cierra contra el plan aprobado para Fase 4:

- el asesor recibe pedidos reales y responde con botones por cliente
- el fluxo detal pasa por confirmacion, domicilio y total final
- la negociacion de hora funciona entre cliente y asesor
- el timeout de asesor habilita reintento o programacion
- los pedidos al mayor y la opcion `Hablar con Asesor` usan relay real
- los timers de asesor y relay deben comportarse correctamente
- `OrderComplete` debe resetear la conversacion sin persistirse como estado final

La evidencia se divide en 4 capas: local automatizada, DB live, WhatsApp live manual y reinicio controlado.

### Precondiciones

- `.env` con `DATABASE_URL`, `TEST_DATABASE_URL`, `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_ID`, `WHATSAPP_VERIFY_TOKEN`, `WHATSAPP_APP_SECRET`, `ADVISOR_PHONE`, `TRANSFER_PAYMENT_TEXT` y `MENU_IMAGE_MEDIA_ID`.
- `ADVISOR_PHONE` debe ser distinto a `WHATSAPP_TEST_RECIPIENT`.
- El webhook de Meta debe apuntar al servicio activo.
- Debe existir un telefono de cliente real para pruebas manuales y el telefono real del asesor.
- Para pruebas locales fuera de horario se puede usar `FORCE_BOGOTA_NOW=2026-03-11 23:30`.
- No usar `FORCE_BOGOTA_NOW` en Railway o produccion.
- Para validar timers largos:
  - timer de asesor: esperar 2 minutos reales o dejar vencer el timer restaurado
  - timer de relay: esperar 30 minutos reales o forzar la expiracion desde DB en ambiente local

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

- [ ] Pedido detal confirmado → asesor recibe resumen + botones con `[...,4567]` o `[...,1234]` segun el cliente.
- [ ] Los `button_id` incluyen el phone completo del cliente, por ejemplo `advisor_confirm_573001234567`.
- [ ] Asesor confirma → pregunta domicilio mencionando `[...,4567]` → digita valor → cliente recibe total final.
- [ ] Asesor `No puedo` → propone hora → ciclo de negociacion funciona, con `[...,4567]` en cada mensaje al asesor.
- [ ] Cliente acepta hora → transiciona a `WaitAdvisorConfirmHour` → asesor recibe `¿Confirma?` → asesor confirma → domicilio.
- [ ] Timer 2 min expira → cliente recibe opciones `Programar`, `Reintentar`, `Menu`.
- [ ] `Reintentar` → se envia de nuevo la notificacion al asesor.
- [ ] Pedido al mayor → asesor toma pedido → relay activo.
- [ ] En relay: mensajes del cliente se reenvian con prefijo `[CLIENTE ...4567]:`.
- [ ] En relay: mensajes del asesor se reenvian al cliente sin prefijo.
- [ ] Asesor finaliza relay → conversacion termina limpiamente → estado vuelve a `MainMenu`.
- [ ] 30 min de inactividad en relay → termina automaticamente → estado vuelve a `MainMenu`.
- [ ] `Hablar con Asesor` → relay o dejar mensaje. Los botones del asesor incluyen `[...,4567]`.
- [ ] Multiples conversaciones pendientes del asesor → los `button_id` identifican cada cliente correctamente.
- [ ] El bot no se confunde si el asesor tiene 2 pedidos pendientes simultaneamente.
- [ ] `OrderComplete` resetea la conversacion a `MainMenu` y no queda persistido como estado final.

### Evidencia automatizada

- `src/bot/states/advisor.rs`
  - parseo de `button_id` del asesor hacia `phone_number`
  - confirmacion inmediata hacia `AskDeliveryCost`
  - confirmacion programada hacia cierre de pedido
  - actualizacion de total final con domicilio
  - timeout con programacion sin perder contexto
  - contacto con asesor y `LeaveMessage`
- `src/bot/states/relay.rs`
  - relay cliente → asesor
  - relay asesor → cliente
  - rechazo de inputs no textuales
  - finalizacion de relay
- `src/bot/timers.rs`
  - timers inician, cancelan y usan el timestamp original
- `tests/phase2_flow.rs`
  - no debe romper el flujo previo mientras Fase 4 queda activa

### Datos sugeridos para pruebas manuales

- Cliente A: `573001234567`
- Cliente B: `573001234568`
- Pedido detal base:
  - `2` con licor
  - pago `Contra Entrega`
  - direccion valida
- Pedido detal con transferencia:
  - `2` con licor
  - `Pago Ahora`
  - imagen de comprobante valida
- Pedido mayor base:
  - `25` sin licor o `20` con licor
- Horas de negociacion sugeridas:
  - propuesta asesor: `5:30 pm`
  - contraoferta cliente: `6:00 pm`

### Validacion manual de WhatsApp

#### Escenario 1: Pedido detal confirmado con domicilio

1. Desde el cliente, entrar por `Hacer Pedido`.
2. Completar pedido detal y confirmar direccion.
3. Verificar en el WhatsApp del asesor:
   - llega resumen del pedido
   - llegan botones con `[...,4567]` en el titulo
   - los `button_id` contienen el phone completo del cliente
4. Desde el asesor, presionar `Confirmar`.
5. Verificar que el asesor reciba una pregunta de domicilio mencionando `[...,4567]`.
6. Desde el asesor, enviar por texto `5000`.
7. Verificar que el cliente reciba subtotal, domicilio y total final.
8. Confirmar en DB:
   - `orders.status = confirmed`
   - `orders.delivery_cost = 5000`
   - `orders.total_final` correcto
9. Confirmar en `conversations` que el estado final del cliente es `main_menu`.

#### Escenario 2: Negociacion de hora

1. Repetir flujo detal hasta `ConfirmAddress`.
2. Desde el asesor, presionar `No puedo`.
3. Verificar que el asesor quede en captura de hora libre.
4. Enviar por texto `5:30 pm`.
5. Verificar que el cliente reciba la propuesta con botones aceptar / rechazar.
6. Elegir `Aceptar`.
7. Verificar que la conversacion del cliente pase por `wait_advisor_confirm_hour`.
8. Verificar que el asesor reciba `¿Confirma?` con identificador `[...,4567]`.
9. Presionar `Confirmar`.
10. Verificar que se solicite domicilio.
11. Enviar `5000`.
12. Verificar cierre correcto y reset a `MainMenu`.

#### Escenario 3: Negociacion de hora con contraoferta del cliente

1. Repetir flujo detal hasta que el asesor proponga una hora.
2. En el cliente, elegir `Rechazar`.
3. Enviar una contraoferta, por ejemplo `6:00 pm`.
4. Verificar que el asesor reciba la hora del cliente con botones `Si, confirmo` / `Otra hora`.
5. Elegir `Otra hora`.
6. Verificar retorno a captura de nueva hora del asesor.
7. Enviar una nueva hora.
8. Repetir hasta que el cliente acepte o el asesor confirme la contraoferta.

#### Escenario 4: Timeout de asesor y reintento

1. Completar un pedido detal hasta `WaitAdvisorResponse`.
2. No responder desde el asesor por 2 minutos.
3. Verificar que el cliente reciba `Programar`, `Reintentar`, `Menu`.
4. Elegir `Reintentar`.
5. Verificar que el asesor reciba nuevamente el mismo caso.
6. Confirmar en DB que:
   - `conversations.state` vuelve a `wait_advisor_response`
   - `state_data.advisor_timer_expired = false`

#### Escenario 5: Timeout de asesor y programacion

1. Repetir timeout del escenario anterior.
2. Elegir `Programar`.
3. Completar `SelectDate`, `SelectTime` y `ConfirmSchedule`.
4. Verificar que no se vuelvan a pedir nombre, telefono o direccion.
5. Verificar que el caso vuelve a `wait_advisor_response` o `wait_advisor_mayor` segun el pedido.
6. Confirmar en DB que se conservaron:
   - `items`
   - `payment_method`
   - `delivery_address`

#### Escenario 6: Pedido al mayor y relay

1. Completar un pedido mayor y confirmar direccion.
2. Verificar que el asesor reciba el resumen y boton `Tomar pedido [...4567]`.
3. Presionar `Tomar pedido`.
4. Verificar que:
   - `orders.status = manual_followup`
   - la conversacion del cliente entra a `relay_mode`
5. Enviar texto desde el cliente.
6. Verificar que el asesor lo recibe con prefijo `[CLIENTE ...4567]:`.
7. Responder desde el asesor con texto libre.
8. Verificar que el cliente lo recibe sin prefijo.
9. Verificar que el boton `Finalizar [...4567]` sigue visible para el asesor.

#### Escenario 7: Finalizacion manual de relay

1. Con relay activo, presionar `Finalizar` desde el asesor.
2. Verificar mensajes de cierre en ambos lados.
3. Confirmar en DB:
   - `conversations.state = main_menu` para el cliente
   - la sesion activa del asesor queda limpia
4. Confirmar que la orden queda en `manual_followup`.

#### Escenario 8: Timeout de relay por inactividad

1. Activar relay.
2. No enviar mensajes por 30 minutos.
3. Verificar cierre automatico.
4. Confirmar en DB:
   - `conversations.state = main_menu`
   - no queda `relay_mode` persistido

#### Escenario 9: Hablar con Asesor

1. Desde `MainMenu`, elegir `Hablar con Asesor`.
2. Si el cliente no tiene datos, completar nombre y telefono.
3. Verificar que el asesor reciba botones `Atender [...4567]` / `No disponible [...4567]`.
4. Ruta A:
   - presionar `Atender`
   - validar relay cliente ↔ asesor
5. Ruta B:
   - dejar expirar el timer o presionar `No disponible`
   - verificar opciones `Dejar mensaje` / `Menu`
   - elegir `Dejar mensaje`
   - enviar texto libre
   - confirmar que el asesor recibe el mensaje con nombre y telefono del cliente

#### Escenario 10: Dos casos simultaneos para el asesor

1. Abrir pedido pendiente del Cliente A.
2. Abrir pedido pendiente del Cliente B.
3. Verificar que el asesor recibe dos casos distintos, cada uno con su `[...,xxxx]` correcto.
4. Presionar un boton del Cliente A y responder texto.
5. Confirmar que el texto afecta solo la conversacion del Cliente A.
6. Luego presionar un boton del Cliente B y responder texto.
7. Confirmar que el texto afecta solo la conversacion del Cliente B.
8. Validar que el bot no mezcla estados, horas ni domicilio entre ambos clientes.

### Validacion de reinicio

#### Reinicio con timer de asesor

1. Llevar una conversacion a `wait_advisor_response`, `wait_advisor_mayor` o `wait_advisor_contact`.
2. Confirmar en DB que `state_data.advisor_timer_started_at` existe.
3. Reiniciar el servicio antes del vencimiento.
4. Verificar que el servicio sube sin error.
5. Esperar el tiempo restante y confirmar que el timeout ocurre correctamente.

#### Reinicio con relay activo

1. Activar `relay_mode`.
2. Confirmar en DB que `state_data.relay_timer_started_at` existe.
3. Reiniciar el servicio.
4. Verificar que:
   - el relay sigue operativo si aun no vence
   - o expira correctamente si el tiempo ya se consumio

### Queries sugeridas para evidencia

Estado de conversacion del cliente:

```sql
SELECT phone_number, state, state_data, customer_name, customer_phone, delivery_address
FROM conversations
WHERE phone_number IN ('573001234567', '573001234568', '<ADVISOR_PHONE>');
```

Ultima orden del cliente:

```sql
SELECT id, conversation_id, delivery_type, payment_method, delivery_cost, total_estimated, total_final, status, created_at
FROM orders
WHERE conversation_id = (
  SELECT id FROM conversations WHERE phone_number = '573001234567'
)
ORDER BY id DESC
LIMIT 1;
```

Items de la orden:

```sql
SELECT order_id, flavor, has_liquor, quantity, unit_price, subtotal
FROM order_items
WHERE order_id = <ORDER_ID>
ORDER BY id ASC;
```

### Evidencia esperada

- Capturas o screen recording de WhatsApp para:
  - resumen al asesor con botones
  - confirmacion con domicilio
  - negociacion de hora
  - timeout de asesor
  - relay mayor
  - finalizacion manual de relay
  - timeout de relay
  - `Hablar con Asesor`
- Captura de payload/log o evidencia que demuestre `button_id` con phone completo.
- Query o captura de DB para:
  - `conversations.state`
  - `conversations.state_data`
  - `orders.status`
  - `orders.delivery_cost`
  - `orders.total_final`
  - `order_items`
- Logs del servicio para:
  - restauracion de timers al arrancar
  - expiracion de timer de asesor
  - expiracion de timer de relay

### Criterio de cierre

- `cargo test` en verde.
- Smoke DB live en verde.
- Smoke de transporte WhatsApp en verde.
- Checklist manual completada con evidencia para:
  - detal confirmado con domicilio
  - negociacion de hora
  - timeout con reintento
  - timeout con programacion
  - pedido mayor con relay
  - cierre manual y automatico de relay
  - `Hablar con Asesor`
  - dos casos simultaneos para el asesor
- Confirmacion en DB de:
  - `confirmed` en detal cerrado
  - `manual_followup` en mayor tomado
  - reseteo de `main_menu` al cerrar
  - no persistencia final de `order_complete`
