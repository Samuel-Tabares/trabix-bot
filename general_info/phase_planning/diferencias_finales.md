Las diferencias principales entre Flow_Design_Diagram.mermaid y Flow_Design_Diagram_v2.mermaid son estas:

  - La v1 es una visión objetivo del negocio; la v2 modela cómo corre hoy el sistema de verdad.
  - La v1 casi no muestra infraestructura; la v2 sí incluye Meta -> webhook -> HMAC -> state machine -> PostgreSQL -> timers.
  - La v1 separa fuerte por detal vs al mayor; la v2 refleja que el flujo de pedido con asesor ahora se decide sobre todo por delivery_type (immediate vs scheduled).
  - En la v1, al asesor para pedido normal le salían 3 botones: Confirmar, No puedo, Proponer; en la v2 eso cambió a:
      - scheduled: solo Confirmar
      - immediate: Confirmar y No puedo
  - En la v1, No puedo y Proponer hora eran ramas distintas; en la v2, No puedo es lo que convierte el pedido inmediato en semi-programado y dispara la negociación de hora.
  - En la v1, el pedido al mayor entraba a relay; en la v2, el relay queda documentado como flujo de Hablar con Asesor, no como ruta principal del pedido normal.
  - La v1 sugiere más uso de imágenes en sabores; la v2 refleja el runtime actual: listas de sabores y una sola imagen del menú en Ver Menú.
  - La v1 no aterriza bien qué datos se guardan; la v2 sí muestra conversations, orders, order_items y qué campos del state_data sostienen el flujo.
  - La v1 no baja a detalle los timers; la v2 sí documenta wait_receipt, advisor_response, relay_inactivity y restore_pending_timers.
  - La v1 deja más genérico el cierre; la v2 deja explícito:
      - scheduled confirmado: se registra y vuelve a MainMenu
      - immediate confirmado: pide domicilio, calcula total final, informa 20-40 minutos, y vuelve a MainMenu
  - La v1 hablaba de un flujo deseado más manual; la v2 ya baja a funciones y pasos reales como handoff_order_after_address_confirmation, wait_advisor_response, ask_delivery_cost,
    offer_hour_to_client, wait_client_hour.
