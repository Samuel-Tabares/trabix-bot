# Runbook Operativo (motor de agente en producción)

Guía corta para operar el bot en Railway. Referencia completa del runtime en
`current_runtime_reference.md`.

## No hay rollback al motor determinista

El toggle `BOT_ENGINE` se eliminó en v1.10.0: el código ya no lo lee y borrar la variable en
Railway **no hace nada**. El motor de agente es el único runtime. Si el agente falla, la
degradación es la que describe "El bot no responde a un cliente" — el asesor recibe el aviso y
atiende manualmente; no hay un motor alterno al que caer.

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

- Conversación completa (ambos carriles, cliente↔bot y bot↔asesor, cualquier motor): consola
  desplegada `https://crm-production-618e.up.railway.app` (password = variable `CRM_PASSWORD`
  del servicio `crm`) o directamente la tabla `message_events` (solo captura desde 2026-07-22).
- Memoria cruda del agente LLM: tabla `agent_case_messages` (una fila por teléfono, JSONB con
  todos los turnos, incluye tool calls).
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

## Ventana de 24h de WhatsApp (ping diario del asesor)

Si nadie escribe al número del asesor durante 24h, Meta empieza a rechazar los mensajes del bot
hacia el asesor (los trata como mensaje de plantilla no aprobado). Prevención: el asesor envía
`✅` (solo el emoji) al bot **una vez al día**, antes de cumplirse las 24h. El bot lo ignora en
silencio (no responde, no toca ningún caso) pero para Meta cuenta como mensaje entrante y renueva
la ventana. Si la ventana ya venció (el asesor dejó de recibir avisos), basta con que envíe
cualquier mensaje al bot para reabrirla.

## Consolas de conversaciones (dos, a propósito y por poco tiempo)

Desde 2026-08-01 hay dos consolas corriendo sobre el mismo Postgres: `crm-web` (la vieja, solo
lectura) y `crm-app` (la nueva, con envío saliente). **`crm-web` se mantiene solo como red de
seguridad** hasta que el envío saliente de `crm-app` esté probado punta a punta con un cliente real.
Apenas eso pase: `railway remove --service crm` y borrar esta sección.

- `crm-app`: `https://crm-app-production-405d.up.railway.app`, servicio `crm-app` del mismo
  proyecto. Deploy manual desde `../crm-app`: `railway up --service crm-app --detach`. Envía
  WhatsApp llamando a `POST /internal/advisor/send` de este bot con el header `X-Internal-Token`
  (mismo valor que `INTERNAL_API_TOKEN` acá). Si el envío empieza a fallar con `not_connected`, lo
  primero a revisar es que esos dos valores sigan siendo idénticos.

### La consola vieja (`crm-web/` en Railway)

- URL: `https://crm-production-618e.up.railway.app` — servicio `crm` en el mismo proyecto
  Railway del bot, lee la misma Postgres por red interna (`DATABASE_URL = ${{Postgres.DATABASE_URL}}`).
- Acceso: un solo operador, password en la variable `CRM_PASSWORD` del servicio `crm` (cambiarla
  ahí invalida las sesiones activas, porque la cookie es un hash del password).
- Deploy: **manual**, no se autodespliega con push. Desde la raíz del repo:
  `railway up --service crm --detach` (el servicio tiene `rootDirectory = crm-web`; requiere
  `PORT=3000` ya configurado — el dominio apunta a ese puerto).
- Si sale 502: revisar que `PORT=3000` siga en las variables del servicio.
- La consola solo muestra `message_events` (captura desde 2026-07-22); si la tabla aún no
  existiera, muestra estado vacío sin romper.

## Control de gasto LLM

- Límite por cliente: 30 llamadas/día (constante `PER_PHONE_DAILY_LIMIT` en `src/ai/budget.rs`).
- Kill-switch global: variable `AGENT_DAILY_LLM_CALL_LIMIT` en Railway (sin definir = sin límite
  global). Al alcanzarlo, todos los casos degradan a mensaje fijo + aviso al asesor.
- Gasto real: consola de Anthropic → Usage. Cada turno de cliente consume 1–8 llamadas Haiku
  (tools encadenadas); un pedido completo típico son ~4 turnos de cliente + 1–2 del asesor.
