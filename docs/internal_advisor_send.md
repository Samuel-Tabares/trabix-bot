# `POST /internal/advisor/send` — contrato

Endpoint interno del bot para que **`crm-app` mande WhatsApp sin volverse un segundo escritor**
sobre la conversación. Implementado en `src/routes/internal.rs` (v1.11.0).

## Por qué existe

La decisión de arquitectura (2026-07-31) fue: `crm-app` es la única superficie de trabajo del
asesor, pero **el bot sigue siendo el único dueño de la sesión de WhatsApp**. Si `crm-app` llamara
la Graph API de Meta directamente habría dos emisores sobre la misma conversación, dos lugares con
credenciales de Meta y dos trazas parciales. Con este endpoint hay un solo emisor, un solo sitio con
las credenciales y una sola traza (`message_events`).

## Autenticación

Header `X-Internal-Token` con el valor de la variable de entorno `INTERNAL_API_TOKEN` del bot.

- La comparación es de tiempo constante.
- Si `INTERNAL_API_TOKEN` **no está configurada**, el endpoint responde `503 not_connected` — queda
  deshabilitado, nunca abierto. Esto es deliberado: es preferible que `crm-app` falle con un error
  claro a que un endpoint que manda WhatsApp quede público si alguien olvida la variable.
- El token debe ser el mismo valor en Railway (bot) y en `crm-app` (`TRABIX_BOT_INTERNAL_TOKEN`).
  Generar con `openssl rand -hex 32`.

## Request

```http
POST /internal/advisor/send
Content-Type: application/json
X-Internal-Token: <INTERNAL_API_TOKEN>

{
  "case_phone": "573001234567",
  "body": "Hola, te confirmo que el pedido sale hoy 👍",
  "sent_by": "samuel"
}
```

| Campo | Tipo | Obligatorio | Notas |
|---|---|---|---|
| `case_phone` | string | sí | E.164 sin `+` (el `+` inicial se tolera y se limpia). Solo dígitos, 10–15. Es el teléfono del **cliente**, no el del asesor. |
| `body` | string | sí | Texto libre. Se hace `trim`; máximo 4096 caracteres (límite de la Cloud API). |
| `sent_by` | string | no | Usuario del CRM que lo envió. Solo se guarda en la traza, no se le muestra al cliente. |

## Respuesta OK

```json
{ "wa_message_id": "wamid.HBgMNTczMDAxMjM0NTY3..." }
```

`200 OK`. `wa_message_id` puede venir `null` si Meta aceptó el envío pero no devolvió id — el
mensaje sí salió, así que **no reintentar** en ese caso.

## Errores

Todos los errores devuelven `{ "code": "...", "message": "..." }`. Los `code` están alineados con
la unión `SendError["code"]` de `crm-app/src/server/inbox/send.ts` para que la consola los mapee
directo a UI.

| HTTP | `code` | Qué pasó | Qué debería hacer `crm-app` |
|---|---|---|---|
| 401 | `unauthorized` | Token ausente o incorrecto | Error de configuración; alertar, no reintentar |
| 503 | `not_connected` | `INTERNAL_API_TOKEN` no configurada en el bot | Error de configuración; alertar, no reintentar |
| 400 | `invalid_request` | JSON malformado, teléfono inválido, body vacío o >4096 | Corregir en la consola; no reintentar |
| 404 | `unknown_case` | No hay conversación para ese número | El caso no existe en el bot; no reintentar |
| 409 | `window_closed` | Pasaron >24h desde el último mensaje del cliente (Meta 131047 / 470) | Mostrarle al asesor que necesita una **plantilla**; no reintentar texto libre |
| 502 | `meta_unavailable` | No se pudo contactar a Meta, o Meta devolvió 5xx | Reintentable con backoff |
| 502 | `meta_error` | Meta rechazó el envío (4xx que no es ventana cerrada) | Mostrar el detalle; no reintentar ciego |
| 500 | `internal_error` | Falla del bot (p. ej. la DB) | Reintentable con backoff |

> `window_closed` es el error importante para el producto: con la pauta de Meta corriendo, muchos
> clientes escriben una vez y responden al día siguiente. Ahí el texto libre **no llega** y el
> asesor tiene que saberlo. Las plantillas de WhatsApp (`tests/e2e/us6-templates.md` en `crm-app`)
> son la continuación natural de esto, después de Phase 3.

## Efectos en el bot

1. Toma el mismo **lock de conversación** que usa el motor (`crate::lock_conversation`): si el
   agente está a mitad de un turno para ese cliente, el mensaje del asesor espera en vez de
   intercalarse.
2. Verifica que exista la conversación (`conversations.phone_number`). Es también el guard que
   impide usar el endpoint para mandarle WhatsApp a un número arbitrario.
3. Envía por `WhatsAppClient::send_text` — el mismo transport que usa todo lo demás.
4. Escribe la traza en `message_events` con `channel='client'`, `actor='advisor'`,
   `payload = {"source":"crm-app","sent_by":...}`. Ese `payload.source` es cómo se distingue un
   mensaje mandado desde la consola de uno relevado por el flujo viejo de `advisor.rs`.
5. Toca `conversations.last_message_at`.

La traza y el `last_message_at` son **best-effort**: si fallan, el endpoint igual responde `200`
porque el mensaje ya salió. Devolver error ahí haría que la consola reintente y el cliente reciba
el mensaje dos veces.

## Toma de control humana con auto-devolución (Fase 2, v1.15.0)

Cada llamada a `advisor_send` (y **solo** a esta, ver por qué abajo) marca
`conversations.human_takeover_until = now + ADVISOR_TAKEOVER_HOURS` (env nueva, default `6`,
ventana deslizante — cada envío nuevo la reemplaza, no la acumula). Mientras esa columna sigue en el
futuro:

- `engine::process_customer_input` **no llama al agente** para ese cliente. El mensaje entrante
  sigue quedando en `message_events` (sigue visible en `crm-app`), pero el bot no le contesta nada.
- Los 4 timers que le mandan algo al cliente (`expire_advisor_timer`, `expire_relay_timer`,
  `expire_conversation_abandon`, `expire_business_hours_timer`) se vuelven no-op mientras dure la
  pausa, igual que la reconciliación de timers vencidos al boot.

**Por qué `advisor_reply` NO dispara esto:** ese endpoint existe para que el asesor destrabe una
pregunta puntual del agente (`confirm_advisor_availability`, `set_manual_delivery_cost`) y el bot
**siga** el checkout automático después. Si también pausara, el bot no podría confirmar el pedido ni
responder al "gracias" del cliente hasta que venza la ventana — rompería el propósito mismo del
endpoint. `sendText` sí es un humano escribiéndole libremente al cliente, por fuera del guion del
bot — eso sí es tomar el caso.

### `POST /internal/advisor/release`

Devuelve la conversación al bot antes de que venza la ventana, sin esperar las 6h. Mismo header
`X-Internal-Token`.

```jsonc
{ "case_phone": "573001234567", "sent_by": "user_id del CRM" }
```

No manda nada a Meta ni escribe `message_events` — solo limpia `human_takeover_until`. Responde
`{"ok": true}`, mismos códigos de error que `/reply` (`unauthorized`, `unknown_case`,
`invalid_request`, `internal_error`).

```bash
curl -i -X POST https://<bot>/internal/advisor/release \
  -H "Content-Type: application/json" \
  -H "X-Internal-Token: $INTERNAL_API_TOKEN" \
  -d '{"case_phone":"573001234567","sent_by":"curl"}'
```

## Lo que este endpoint NO hace (todavía)

- **No manda plantillas, ni imágenes, ni botones.** Solo texto libre dentro de la ventana de 24h.
- **No toca `advisor.rs` ni `relay.rs`.** El flujo viejo (bot → WhatsApp del asesor) sigue intacto
  y en producción; este endpoint es un camino nuevo en paralelo.

## Prueba manual

```bash
curl -i -X POST https://<bot>/internal/advisor/send \
  -H "Content-Type: application/json" \
  -H "X-Internal-Token: $INTERNAL_API_TOKEN" \
  -d '{"case_phone":"573001234567","body":"prueba desde crm-app","sent_by":"curl"}'
```

Verificar después: el cliente recibe el mensaje en WhatsApp, y aparece una fila en `message_events`
con `actor='advisor'`, `channel='client'` y `payload->>'source' = 'crm-app'`.

---

# `POST /internal/advisor/reply` (v1.12.0)

El **otro** endpoint, y la distinción importa más de lo que parece.

| | `/internal/advisor/send` | `/internal/advisor/reply` |
|---|---|---|
| A quién le habla | al **cliente** | al **bot** |
| Pasa por el agente | no, texto crudo | **sí**, turno de agente |
| Para qué sirve | escribirle al cliente | contestarle una pregunta al bot |

El bot le hace preguntas al asesor que son **pasos bloqueantes del flujo de pedido**:

- *"¿puedes entregar ya?"* → el agente espera para llamar `confirm_advisor_availability`.
- *"¿cuánto vale el domicilio a este municipio?"* → espera para llamar `set_manual_delivery_cost`.

Esas respuestas tienen que entrar **por el agente**. Si el asesor contesta con `send`, el cliente
recibe un texto suelto pero el pedido queda colgado esperando una respuesta que nunca llega. Por eso
existe `reply`.

## Request

```jsonc
{
  "case_phone": "573001234567",  // el caso; la consola ya sabe dónde está parada
  "body": "sí, puedo entregar en 40 minutos",
  "sent_by": "user_id del CRM"   // solo traza
}
```

Mismo header `X-Internal-Token`, mismos códigos de error (`unauthorized`, `unknown_case`,
`invalid_request`, `internal_error`). Diferencias:

- **No devuelve `wa_message_id`.** Un turno de agente puede generar cero, uno o varios mensajes al
  cliente — no hay un único id. Devuelve `{"ok": true}`.
- **No devuelve `window_closed`.** Lo que se manda no va directo a Meta; los mensajes al cliente los
  produce el agente y su envío se traza aparte.

## El asesor no tiene canal directo de WhatsApp

El bot nunca le manda WhatsApp al asesor: cualquier pregunta/aviso sobre un caso (`message_advisor`,
auto-aceptación, confirmación de pago, etc.) se escribe directo en `message_events` con
`channel='advisor'` (`BotAction::NotifyAdvisor`, ver `src/engine.rs`/`src/ai/agent.rs`) sin pasar por
Meta. La consola levanta esas filas de ahí (el caso queda marcado `needs_human`) y el asesor contesta
con este endpoint. `ADVISOR_WHATSAPP_ENABLED` ya no existe — el corte es permanente, no un flag. Lo
que queda de `ADVISOR_PHONE` en el código es residuo del FSM determinístico heredado
(`src/bot/states/advisor.rs`/`relay.rs`), no del motor de agente — ver
`docs/CLEANUP_deterministic_engine.md` §3.

## `GET /internal/media/:media_id` — proxy de adjuntos (v1.23.0)

`crm-app` no tiene credenciales de Meta propias (a propósito, mismo principio de "un solo dueño de
la sesión de WhatsApp"), así que no puede resolver ni descargar un adjunto (imagen de comprobante,
etc.) directamente desde la Graph API. Este endpoint lo hace por él: mismo header
`X-Internal-Token`, responde los bytes crudos con el `Content-Type` que reporta Meta (`image/jpeg`,
etc.) y `Cache-Control: private, max-age=86400`. El `media_id` sale de `payload.media_id` en la fila
de `message_events` con `content_type='image'`.

No valida que el `media_id` pertenezca a un caso conocido: son IDs opacos de Meta de un solo uso
práctico (expiran solos), sin valor fuera de este contexto — no hace falta ese chequeo extra.

```bash
curl -i "https://<bot>/internal/media/<media_id>" \
  -H "X-Internal-Token: $INTERNAL_API_TOKEN" \
  -o comprobante.jpg
```

## Prueba manual

```bash
curl -i -X POST https://<bot>/internal/advisor/reply \
  -H "Content-Type: application/json" \
  -H "X-Internal-Token: $INTERNAL_API_TOKEN" \
  -d '{"case_phone":"573001234567","body":"sí, puedo entregar ya","sent_by":"curl"}'
```
