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
   `payload = {"source":"crm-app","sent_by":...}`.
5. **Apenda el mensaje a la memoria del agente** (`agent_case_messages`) como una línea de
   transcript marcada por el sistema, sin llamar al LLM — ver
   `ai::memory::append_transcript_entry`. Es la mitad que faltaba del transcript del handoff: aquí
   es donde se dice el valor del envío o que el pago sí llegó, y es lo que el turno de recuperación
   lee cuando le devuelven la conversación. Sin esto el bot volvía con un hueco justo donde se
   resolvió lo importante.
6. Toca `conversations.last_message_at`.

La traza y el `last_message_at` son **best-effort**: si fallan, el endpoint igual responde `200`
porque el mensaje ya salió. Devolver error ahí haría que la consola reintente y el cliente reciba
el mensaje dos veces.

## Toma de control humana con auto-devolución

Cada llamada a `advisor_send` marca `conversations.human_takeover_until = now +
ADVISOR_TAKEOVER_HOURS` (default `6`, ventana deslizante — cada envío nuevo la reemplaza, no la
acumula). El bot también se la pone a sí mismo cuando hace un handoff
(`BotAction::HandOffToHuman`). Mientras esa columna sigue en el futuro:

- `engine::process_customer_input` **no llama al agente** para ese cliente. El mensaje entrante
  sigue quedando en `message_events` (sigue visible en `crm-app`) y además se apenda a la memoria
  del agente, pero el bot no le contesta nada.
- Los timers que le mandan algo al cliente (`expire_receipt_timer`,
  `expire_business_hours_timer`) se vuelven no-op mientras dure la pausa, igual que la
  reconciliación de timers vencidos al boot.

### `POST /internal/advisor/release` — y el turno de recuperación

Devuelve la conversación al bot antes de que venza la ventana, sin esperar las 6h. Mismo header
`X-Internal-Token`.

```jsonc
{ "case_phone": "573001234567", "sent_by": "user_id del CRM" }
```

**Desde v1.27.0 esto hace dos cosas, no una:**

1. Limpia `human_takeover_until`.
2. **Dispara el turno de recuperación** (`engine::process_resume_for_case` →
   `ai::agent::run_resume_turn`). El bot lee el transcript del handoff —que se fue apendando en
   vivo por los dos lados— y sigue el pedido desde ahí: fija con `set_manual_delivery_cost` el
   costo que cotizó el asesor, confirma con `confirm_payment_received` el pago que verificó, cierra
   el checkout, y le reporta al asesor por `message_advisor` qué concluyó.

Ese reporte de cierre es el ancla contra el riesgo del diseño: el modelo lee cifras de dinero de un
chat en prosa, así que si leyó mal aparece de inmediato en Pendientes.

Un fallo del agente **no** hace fallar la liberación: la toma de control ya quedó levantada, que es
lo que pidió el asesor, y `degrade_agent_failure` deja el aviso en el carril del asesor. Por eso
`crm-app` llama este endpoint con el timeout largo de turno de agente (60s), no con el de un POST a
Meta.

Si nadie pulsa "Devolver al bot", la ventana vence sola a las 6h y el barrido de 60s
(`bot::timers::sweep_expired_handoffs`) corre el mismo turno de recuperación — pero solo para los
casos con `state_data.handoff_reason` puesto, o sea los que el bot entregó. Una toma de control que
un humano inició por su cuenta no tiene nada que retomar.

Responde `{"ok": true}`, mismos códigos de error que `/send` (`unauthorized`, `unknown_case`,
`invalid_request`, `internal_error`).

```bash
curl -i -X POST https://<bot>/internal/advisor/release \
  -H "Content-Type: application/json" \
  -H "X-Internal-Token: $INTERNAL_API_TOKEN" \
  -d '{"case_phone":"573001234567","sent_by":"curl"}'
```

## Lo que este endpoint NO hace (todavía)

- **No manda plantillas, ni imágenes, ni botones.** Solo texto libre dentro de la ventana de 24h.
  Con el carril del bot fuera, esto se vuelve más áspero: el asesor *tiene* que escribirle al
  cliente, así que una ventana de 24h cerrada bloquea el handoff. Samuel sacó las plantillas del
  backlog el 2026-08-25; queda como limitación conocida.

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

# El carril asesor→bot: eliminado (v1.27.0)

Existía un segundo endpoint, `POST /internal/advisor/reply`, que metía el texto del asesor en un
turno de agente para que contestara preguntas bloqueantes del bot ("¿cuánto vale el domicilio a
este municipio?"). **Ya no existe**, y con él se fueron `Actor::Advisor`, `run_advisor_turn`,
`process_advisor_turn_for_case`, el timer `AdvisorResponse` y todo el FSM determinista de
asesor/relay.

El motivo fue un incidente que se repitió dos veces. En un turno de asesor, el texto plano del
modelo iba al asesor, no al cliente: el 2026-09-11, con el cliente Graja, el asesor contestó `28000`
y el "total $271.000, falta la dirección" se quedó atrapado en el carril interno. El cliente nunca lo
vio y la venta se cerró a mano. El primer parche (2026-08-12) solo cubrió la rama en la que ya
existe un pedido finalizado.

**El modelo nuevo:** el asesor solo le habla al cliente. El bot, cuando se topa con algo que no
puede resolver, hace un handoff determinista y se sale; cuando le devuelven la conversación, lee lo
que se dijo y sigue. Detalle en la sección de `/release` arriba.

## El asesor no tiene canal directo de WhatsApp

El bot nunca le manda WhatsApp al asesor: cualquier nota sobre un caso (`message_advisor`, un
handoff, una auto-aceptación) se escribe directo en `message_events` con `channel='advisor'`
(`BotAction::NotifyAdvisor`, ver `src/engine.rs`/`src/ai/agent.rs`) sin pasar por Meta. La consola
levanta esas filas de ahí y el caso queda marcado `needs_human`. `message_advisor` es una nota de
**una sola vía**: nadie le responde al bot por ahí.

Lo que queda de `ADVISOR_PHONE` en el código es residuo de la clasificación de carriles
(`engine::channel_for_recipient`), no un canal real.

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
