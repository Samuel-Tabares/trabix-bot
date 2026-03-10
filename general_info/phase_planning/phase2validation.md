## Validación de Fase 2

  ### Resumen

  Validar Fase 2 contra el plan aprobado, no contra la checklist original de la spec completa. Eso implica estos criterios funcionales:

  - ViewMenu y selección de sabores se validan con texto provisional, no con imagen.
  - El flujo termina en ShowSummary con resumen textual básico.
  - No se validan precios, pagos, timers ni asesor real en esta fase.

  La validación debe producir evidencia en 3 capas: unit/local, DB live, y WhatsApp live.

  ### Cambios de validación a preparar

  - Consolidar una matriz de aceptación basada en el flujo realmente aprobado:
      - hola en main_menu muestra lista interactiva con 4 opciones.
      - Ver Menú muestra texto de precios + botones make_order y back_main_menu.
      - Horarios muestra texto + botones make_order y back_main_menu.
      - Hacer Pedido permite immediate_delivery y scheduled_delivery.
      - OutOfHours muestra schedule_later, contact_advisor_now, back_main_menu.
      - SelectDate, SelectTime, ConfirmSchedule validan fecha/hora y reintentan ante input inválido.
      - CollectName, CollectPhone, CollectAddress validan y persisten columnas de conversations.
      - SelectType, SelectFlavor, SelectQuantity, AddMore soportan loop completo y mezcla de items.
      - finish_order llega a ShowSummary textual.
  - Extender la validación automatizada para cubrir los huecos que hoy no están probados explícitamente:
      - reintentos por input inválido en ViewMenu, ViewSchedule, WhenDelivery, OutOfHours, AddMore
      - persistencia/rehidratación de pending_has_liquor, pending_flavor, items, scheduled_date, scheduled_time
      - ShowSummary como cierre de Fase 2 y retorno a menú si aplica
      - persistencia de customer_name, customer_phone, delivery_address en DB
  - Mantener tests/live_db.rs y agregar un smoke test específico de Fase 2 en DB:
      - crear conversación
      - persistir collect_address, select_quantity, add_more, show_summary
      - recargar y comprobar que state y state_data sobreviven intactos
      - comprobar también columnas customer_name, customer_phone, delivery_address
  - Agregar una validación manual live de WhatsApp orientada a flujo, no solo transporte:
      - hoy tests/live_whatsapp.rs solo valida send_text/send_buttons/send_list
      - la validación de Fase 2 debe hacerse contra el webhook real, enviando mensajes desde un tester y verificando respuestas del bot

  ### Ejecución de validación

  1. Exportar variables del .env antes de cualquier smoke test live.
      - Usar set -a; source .env; set +a
      - Esto es obligatorio porque los tests live ignorados no cargan .env por sí solos.
  2. Correr base local:
      - cargo test
      - resultado esperado: suite local completa en verde
  3. Correr smoke DB live:
      - cargo test --test live_db -- --ignored --test-threads=1
      - resultado esperado: migraciones, CRUD básico y persistencia de Fase 2 en verde
  4. Ejecutar validación manual end-to-end con WhatsApp real sobre el webhook activo.
      - Precondición asumida: Meta sigue apuntando al servicio activo del bot
      - Si no está apuntando al servicio actual, reconfigurar temporalmente el webhook antes de esta validación
  5. Registrar evidencia por cada caso:
      - captura o transcripción de mensajes de WhatsApp
      - estado esperado en PostgreSQL
      - resultado observado

  ### Casos y evidencia esperada

  - hola:
      - respuesta: bienvenida + lista interactiva de 4 opciones
      - DB: conversación creada con state = main_menu
  - Ver Menú:
      - respuesta: texto de precios provisional + botones
      - DB: state = view_menu
  - Horarios:
      - respuesta: texto de horarios + botones
      - DB: state = view_schedule
  - Pedido inmediato dentro de horario:
      - respuesta: avanza a CollectName
      - DB: delivery_type = immediate, state = collect_name
  - Fuera de horario:
      - respuesta: opciones schedule_later, contact_advisor_now, back_main_menu
      - DB: state = out_of_hours
  - Pedido programado:
      - fecha inválida: repite pregunta
      - fecha futura válida: pasa a hora
      - hora inválida: repite pregunta
      - hora válida + confirmar: pasa a CollectName
  - Recolección:
      - nombre corto, teléfono con letras, dirección corta: todos deben reintentar
      - valores válidos: deben persistirse en columnas de conversations
  - Selección de items:
      - soportar al menos 3 items mixtos con/sin licor
      - tras cada cantidad válida, debe verse resumen parcial y botones add_more / finish_order
      - DB: items acumulados en state_data
  - Persistencia entre reinicios:
      - detener servicio con conversación en curso
      - reiniciar
      - enviar siguiente mensaje
      - resultado esperado: continúa en el estado correcto sin perder state_data
  - ShowSummary:
      - debe mostrar resumen textual básico
      - no debe calcular precios ni ofrecer pagos reales en esta fase

  ### Supuestos y defaults

  - El criterio oficial de cierre es el plan aprobado de Fase 2, no la spec completa original.
  - ViewMenu no requiere imagen en esta validación; si se exige imagen, eso ya sería un gap funcional a corregir antes del cierre.
  - Se usará el número de prueba de Meta y el PostgreSQL definido en .env.
  - La validación live de WhatsApp será manual end-to-end; los tests automáticos live existentes no sustituyen esa prueba porque no ejercitan webhook.rs.