## Validacion de Fase 2

### Resumen

Esta validacion se cierra contra el plan aprobado para Fase 2:

- `ViewMenu` y seleccion de sabores usan texto provisional, no imagen.
- El flujo llega hasta `ShowSummary` con resumen textual basico.
- No se validan precios, pagos, timers ni asesor real.

La evidencia se divide en 3 capas: local automatizada, DB live y WhatsApp live manual.

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

Nota:

- Los tests live ahora intentan cargar `.env` automaticamente con `dotenvy`.
- Si ejecutas fuera de la raiz del repo o sin `.env`, exporta variables manualmente.

### Matriz de aceptacion

- `hola` en `main_menu` responde con bienvenida y lista interactiva de 4 opciones.
- `Ver Menu` responde con texto de precios provisional y botones `make_order` / `back_main_menu`.
- `Horarios` responde con texto de horarios y botones `make_order` / `back_main_menu`.
- `Hacer Pedido` permite `immediate_delivery` y `scheduled_delivery`.
- `OutOfHours` responde con `schedule_later`, `contact_advisor_now`, `back_main_menu`.
- `SelectDate`, `SelectTime` y `ConfirmSchedule` reintentan con inputs invalidos y persisten fecha/hora validas.
- `CollectName`, `CollectPhone` y `CollectAddress` validan y persisten columnas de `conversations`.
- `SelectType`, `SelectFlavor`, `SelectQuantity` y `AddMore` soportan loop completo y mezcla de items.
- `finish_order` llega a `ShowSummary`.
- Reiniciar el flujo entre mensajes no pierde `state` ni `state_data`.

### Evidencia automatizada

- `tests/phase2_flow.rs`
  - cubre saludo inicial, menu, horarios, programacion, recoleccion, mezcla de 3 items, llegada a `ShowSummary` y retorno a menu
  - simula persistencia y rehidratacion entre mensajes serializando `state_data` en cada paso
- `tests/live_db.rs`
  - valida migraciones y CRUD basico
  - valida persistencia de progreso Fase 2, incluyendo columnas del cliente y `state_data`
- `tests/live_whatsapp.rs`
  - mantiene smoke test de `send_text`, `send_buttons` y `send_list`

### Validacion manual de WhatsApp

Precondicion:

- El webhook de Meta debe apuntar al servicio activo del bot.

Checklist manual:

1. Enviar `hola` y confirmar lista de 4 opciones.
2. Navegar a `Ver Menu` y verificar texto + botones.
3. Volver al menu.
4. Navegar a `Horarios` y verificar texto + botones.
5. Entrar a `Hacer Pedido`, probar inmediata o programada.
6. Probar errores de nombre, telefono, direccion, fecha, hora y cantidad.
7. Agregar al menos 3 items mixtos.
8. Finalizar y verificar `ShowSummary`.
9. Consultar PostgreSQL y confirmar `state`, columnas de cliente e `items` en `state_data`.
10. Reiniciar el servicio en mitad del flujo y confirmar que la conversacion continua.

### Criterio de cierre

- `cargo test` en verde.
- Smoke DB live en verde.
- Smoke de transporte WhatsApp en verde.
- Checklist manual end-to-end completada con evidencia.
