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

## Lo que este endpoint NO hace (todavía)

- **No cambia el estado de la conversación.** El bot sigue en el estado donde estaba y va a seguir
  respondiéndole al cliente. Silenciar al bot cuando un humano toma el caso es Phase 4, no esto.
- **No pausa el timer de inactividad.** Si hay un recordatorio pendiente, va a dispararse aunque el
  asesor esté escribiendo. También es Phase 4.
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
