  # Fase 4: Interacción con asesor, confirmación y relay

  ## Resumen

  Partimos de una Fase 3 ya operativa:

  - el cliente completa pedido, pago y confirmación de dirección
  - el pedido queda persistido en orders / order_items
  - la conversación termina en pending_advisor / WaitAdvisorResponse
  - advisor.rs y relay.rs existen, pero siguen como stubs
  - webhook.rs todavía no enruta mensajes del asesor al flujo real

  La Fase 4 convierte ese handoff en operación real: el asesor recibe pedidos, confirma con domicilio, negocia hora, toma pedidos al por mayor por relay y atiende la opción
  “Hablar con Asesor”.

  ## Cambios de Implementación

  ### 1. Enrutamiento real del asesor y sesión activa

  - Dividir el procesamiento del webhook en dos rutas:
      - cliente: flujo actual
      - asesor: resolución del cliente objetivo y transición sobre la conversación de ese cliente
  - Implementar handle_advisor_message() en webhook.rs.
      - Si el input del asesor es botón, extraer el phone_number del button_id
      - Si es texto libre, usar una sesión activa temporal del asesor
  - Persistir una sesión activa única del asesor en la conversación del ADVISOR_PHONE.
      - nuevo campo en state_data: advisor_target_phone: Option<String>
      - esa sesión se actualiza al presionar un botón de un cliente y se limpia al terminar el paso de texto libre
  - Si el asesor escribe texto sin sesión activa válida, el bot le responde que primero debe usar un botón de un caso pendiente.
  - Regla fija para títulos del asesor: siempre incluir [...,4567] usando los últimos 4 dígitos del phone_number del cliente.
  - Regla fija para button_id: siempre incluir el phone completo del cliente.
      - advisor_confirm_<phone>
      - advisor_cannot_<phone>
      - advisor_propose_<phone>
      - advisor_yes_hour_<phone>
      - advisor_other_hour_<phone>
      - advisor_take_<phone>
      - advisor_finish_<phone>
      - advisor_attend_<phone>
      - advisor_unavailable_<phone>

  ### 2. Estados, contexto y timers de Fase 4

  - Activar de verdad estos estados ya existentes:
      - WaitAdvisorResponse
      - AskDeliveryCost
      - NegotiateHour
      - OfferHourToClient
      - WaitClientHour
      - WaitAdvisorHourDecision
      - WaitAdvisorConfirmHour
      - WaitAdvisorMayor
      - RelayMode
      - WaitAdvisorContact
      - LeaveMessage
  - Extender ConversationStateData y ConversationContext con:
      - advisor_target_phone: Option<String> para la conversación del asesor
      - advisor_timer_started_at: Option<DateTime<Utc>>
      - advisor_timer_expired: bool
      - relay_timer_started_at: Option<DateTime<Utc>>
      - relay_kind: Option<String> con valores wholesale_order o contact_advisor
      - advisor_proposed_hour: Option<String>
      - client_counter_hour: Option<String>
      - schedule_resume_target: Option<String> con valores wait_advisor_response o wait_advisor_mayor
  - Activar TimerType::AdvisorResponse y TimerType::RelayInactivity.
  - Restaurar ambos timers tras reinicio usando timestamps explícitos en state_data, igual que ya se hizo con WaitReceipt.

  ### 3. Ruta detal y programada con asesor

  - Al confirmar dirección en ConfirmAddress:
      - si el pedido es todo detal, dejar orders.status = pending_advisor
      - enviar resumen al asesor
      - si el pago fue por transferencia, enviar también la imagen del comprobante al asesor como mensaje aparte
      - enviar al asesor botones Confirmar / No puedo / Proponer hora
      - enviar al cliente mensaje de espera
      - iniciar timer de 2 minutos
      - transicionar a WaitAdvisorResponse
  - WaitAdvisorResponse bloquea salida del cliente mientras el asesor no responda.
      - no se permite menu ni cancelación
      - inputs del cliente solo reciben mensaje de “estamos confirmando disponibilidad”
  - Si el asesor presiona Confirmar:
      - cancelar timer
      - si el pedido es immediate, preguntar domicilio y pasar a AskDeliveryCost
      - si el pedido es scheduled, no pedir domicilio en esta fase:
          - marcar orders.status = confirmed
          - avisar al cliente que el pedido programado quedó registrado
          - pasar a OrderComplete
  - En AskDeliveryCost, el siguiente texto libre del asesor debe ser un número entero positivo.
      - si no es válido, pedirlo otra vez
      - al recibirlo:
          - calcular total_final = total_estimated + delivery_cost
          - guardar delivery_cost, total_final y status = confirmed
          - enviar confirmación final al cliente
          - transicionar a OrderComplete

  ### 4. Negociación de hora

  - No puedo y Proponer hora llevan al mismo paso operativo: pedir hora libre al asesor y pasar a NegotiateHour.
  - En NegotiateHour:
      - guardar el texto en advisor_proposed_hour
      - enviar al cliente propuesta con botones Aceptar / Rechazar
      - transicionar a OfferHourToClient
  - Si el cliente acepta:
      - enviar al asesor botón Confirmar
      - transicionar a WaitAdvisorConfirmHour
  - Si el cliente rechaza:
      - pedir al cliente su hora
      - guardar en client_counter_hour
      - enviar al asesor botones Sí, confirmo / Otra hora
      - transicionar a WaitAdvisorHourDecision
  - Si el asesor confirma la contraoferta del cliente:
      - para immediate, ir a AskDeliveryCost
      - para scheduled, cerrar como pedido registrado y OrderComplete
  - Si el asesor elige Otra hora, volver a NegotiateHour.
  - La validación de hora en Fase 4 sigue flexible: texto no vacío dentro del rango de longitud ya usado por el sistema; no introducir parseo estricto nuevo.

  ### 5. Ruta al mayor y relay

  - Si el pedido incluye mayor (es_mayor_con_licor o es_mayor_sin_licor):
      - al confirmar dirección, enviar resumen al asesor con botón Tomar pedido
      - enviar aviso al cliente
      - iniciar timer de 2 minutos
      - transicionar a WaitAdvisorMayor
  - WaitAdvisorMayor también bloquea salida del cliente mientras no haya respuesta del asesor.
  - Si el asesor toma el pedido:
      - cancelar timer
      - marcar orders.status = manual_followup
      - fijar relay_kind = wholesale_order
      - entrar a RelayMode
      - enviar mensaje de inicio a cliente y asesor
      - iniciar timer de inactividad de 30 minutos
      - enviar botón Finalizar al asesor como mensaje separado
  - Relay v1 será solo texto.
      - cliente en RelayMode + texto: reenviar al asesor con prefijo [CLIENTE ...4567]:
      - asesor con sesión activa hacia un cliente en RelayMode + texto: reenviar al cliente sin prefijo
      - cada mensaje reinicia el timer de 30 minutos
      - después de cada mensaje reenviado al asesor, volver a enviar el botón Finalizar
      - si llega imagen, lista, botón u otro tipo no soportado, responder al emisor que el relay de esta fase solo admite texto
  - Si el asesor presiona Finalizar o expira el timer:
      - cerrar relay para ambos lados
      - limpiar sesión del asesor
      - resetear la conversación del cliente
      - no cambiar el status de la orden después de manual_followup

  ### 6. Timeout del asesor y reprogramación

  - Si expira el timer de 2 minutos en WaitAdvisorResponse o WaitAdvisorMayor:
      - marcar advisor_timer_expired = true
      - enviar al cliente botones Programar / Reintentar / Menú
  - Mientras el timer no haya expirado, esos estados no permiten salida ni cancelación.
  - Si el cliente elige Reintentar:
      - reenviar exactamente la misma notificación al asesor
      - reiniciar timer
      - limpiar advisor_timer_expired
  - Si el cliente elige Programar:
      - guardar schedule_resume_target
      - entrar a SelectDate
      - reutilizar el flujo de fecha/hora, pero al confirmar no volver a captura de datos:
          - actualizar delivery_type = scheduled
          - conservar items, pago y dirección
          - reenviar al asesor
          - volver a WaitAdvisorResponse o WaitAdvisorMayor según el target guardado
  - Si el cliente elige Menú:
      - resetear conversación
      - si hay orden abierta, marcarla cancelled

  ### 7. Opción “Hablar con Asesor”

  - Mantener el flujo de entrada actual, pero completar la segunda mitad.
  - Si el cliente ya tiene nombre y teléfono en contexto, saltar directamente al envío al asesor.
  - Al entrar en espera de asesor:
      - notificar al asesor con botones Atender / No disponible
      - enviar mensaje de espera al cliente
      - iniciar timer de 2 minutos
      - transicionar a WaitAdvisorContact
  - Si el asesor presiona Atender:
      - cancelar timer
      - fijar relay_kind = contact_advisor
      - entrar a RelayMode
      - no crear ni tocar órdenes
  - Si el asesor presiona No disponible o expira el timer:
      - cliente recibe Dejar mensaje / Menú
      - transicionar a LeaveMessage si decide dejar mensaje
  - En LeaveMessage:
      - aceptar un texto libre del cliente
      - reenviarlo al asesor con nombre y teléfono
      - resetear conversación a menú

  ## Cambios de interfaces y helpers

  - webhook.rs
      - separar handle_client_message() y handle_advisor_message()
      - soportar expiración real de AdvisorResponse y RelayInactivity
  - BotAction
      - agregar acciones para persistir y limpiar la sesión activa del asesor
      - activar RelayMessage como envío real de texto
  - db/queries.rs
      - reutilizar get_conversation, update_state, update_order_status, update_order_delivery_cost
      - agregar solo helpers pequeños si hacen falta para sesión activa del asesor o cierre de relay
  - advisor.rs
      - pasar de stub a generador real de resúmenes, botones y transiciones de asesor
  - relay.rs
      - implementar entrada, reenvío, botón de cierre y timeout
  - No se requiere migración SQL: los cambios viven en state_data JSON y en nuevos valores de orders.status.

  ## Pruebas

  - Unit tests:
      - parseo de button_id del asesor hacia phone_number
      - bind y clear de advisor_target_phone
      - confirmación detal inmediata pasa a AskDeliveryCost
      - confirmación programada cierra sin domicilio
      - cálculo de total_final con domicilio
      - negociación completa: asesor propone, cliente acepta, cliente rechaza, asesor contraoferta
      - timeout de asesor en detal y mayor
      - Programar desde timeout reutiliza fecha/hora sin perder items ni pago
      - relay texto cliente→asesor y asesor→cliente
      - relay rechaza inputs no textuales
      - Hablar con Asesor atiende, hace timeout y deja mensaje
  - Integración local:
      - webhook enruta mensajes del asesor a la conversación correcta del cliente
      - dos pedidos pendientes simultáneos no colisionan por button_id
      - restauración tras reinicio de timers de AdvisorResponse y RelayInactivity
      - actualización de orders.status, delivery_cost y total_final
  - Validación manual:
      - detal confirmado con domicilio
      - detal con negociación de hora
      - timeout del asesor con Reintentar
      - timeout del asesor con Programar
      - pedido al mayor tomado por relay
      - relay finalizado por asesor
      - relay finalizado por timeout
      - “Hablar con Asesor” atendido
      - “Hablar con Asesor” con timeout y mensaje

  ## Supuestos y defaults

  - El asesor trabaja con una sola sesión activa de texto libre a la vez, ligada al último botón de cliente que presionó.
  - Relay v1 soporta solo texto.
  - Mientras un cliente espera respuesta del asesor, no puede salir con menu ni cancelar; el bot bloquea la salida hasta respuesta, timeout o cierre del relay.
  - Los pedidos programados confirmados por el asesor se marcan confirmed y se cierran sin cálculo automático de domicilio en esta fase.
  - Los pedidos al mayor pasan a manual_followup cuando el asesor toma el caso; el resultado comercial final ocurre fuera del bot.
  - Los últimos 4 dígitos visuales siempre salen del phone_number del cliente en WhatsApp, no del customer_phone capturado.
  - OrderComplete sigue siendo un estado transitorio que resetea a MainMenu; no debe quedar persistido como estado estable.