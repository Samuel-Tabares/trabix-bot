# Runbook Operativo (modo agente en producción)

Guía corta para operar el bot con `BOT_ENGINE=agent` en Railway. Referencia completa del
runtime en `current_runtime_reference.md`.

## Rollback inmediato al motor determinista

1. En Railway → servicio del bot → Variables: **eliminar** `BOT_ENGINE` (o ponerla en
   `deterministic`).
2. Redeploy. Nada más: las tablas son compartidas, no hay migración de vuelta.
3. Las conversaciones que estaban en estados del agente (`main_menu`, `ask_delivery_cost`,
   `select_payment_method`, `wait_receipt`, etc.) siguen funcionando con los handlers
   deterministas de esos mismos estados.

## El bot no responde a un cliente

1. Logs de Railway: buscar `agent turn failed` (falla del LLM) o `LLM daily budget exhausted`
   (límite diario). En ambos casos el asesor ya recibió aviso por WhatsApp con el contexto.
2. Si es falla del LLM: revisar el status de Anthropic y el saldo de la consola. El caso queda
   donde estaba; cuando la API vuelva, el bot retoma solo con el siguiente mensaje del cliente.
3. Si es límite diario: atender al cliente manualmente. El contador se reinicia a medianoche de
   Bogotá (o con un redeploy, porque vive en memoria).
4. Si no hay logs del mensaje: problema de webhook — verificar en Meta que la app esté `Live`,
   la WABA suscrita (`GET /{WABA_ID}/subscribed_apps`) y el callback exacto `/webhook`.

## Leer un caso atascado

- Transcripción completa del agente: tabla `agent_case_messages` (una fila por teléfono, JSONB
  con todos los turnos) o el dashboard local `crm-web/`.
- Estado actual: tabla `conversations` → columnas `state` y `state_data` (incluye
  `current_order_id`, timers, costo de domicilio, método de pago).
- Pedido: tablas `orders` / `order_items`. Estados relevantes: `pending_advisor` (esperando
  confirmación), `draft_payment` (confirmado, falta pago), `waiting_receipt`, `confirmed`,
  `cancelled`, `manual_followup` (timer de asesor venció; atender por fuera).
- Para destrabar un caso sin tocar la BD: que el CLIENTE escriba cualquier mensaje (el agente
  relee el estado en cada turno). Como último recurso: `UPDATE conversations SET state =
  'main_menu', state_data = '{}' WHERE phone_number = '<tel>';` (pierde el carrito, no los
  pedidos ya persistidos).

## Re-subir la imagen del menú (media_id vencido)

```bash
cargo run --bin upload_media -- /ruta/al/menu.jpg
```

Copiar el `media_id` impreso a la variable `MENU_IMAGE_MEDIA_ID` en Railway y redeploy.

## Control de gasto LLM

- Límite por cliente: 60 llamadas/día (constante `PER_PHONE_DAILY_LIMIT` en `src/ai/budget.rs`).
- Kill-switch global: variable `AGENT_DAILY_LLM_CALL_LIMIT` en Railway (sin definir = sin límite
  global). Al alcanzarlo, todos los casos degradan a mensaje fijo + aviso al asesor.
- Gasto real: consola de Anthropic → Usage. Cada turno de cliente consume 1–8 llamadas Haiku
  (tools encadenadas); un pedido completo típico son ~4 turnos de cliente + 1–2 del asesor.

## Checklist del canario (antes de abrir al público)

- [ ] 0 clientes en silencio (todo error visible tuvo mensaje fijo + aviso al asesor)
- [ ] 0 totales incorrectos en mensajes al cliente (comparar transcripciones vs. `orders.total_final`)
- [ ] `orders` confirmadas = conversaciones que terminaron en confirmación
- [ ] `customers` y `referral_code_analytics` cuadran con los pedidos confirmados
- [ ] Costo por conversación medido en la consola de Anthropic y aceptado
- [ ] Rollback probado una vez (quitar `BOT_ENGINE` → determinista responde)
