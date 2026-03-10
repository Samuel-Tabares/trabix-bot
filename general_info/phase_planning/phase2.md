## Resumen

    Partimos de una Fase 1 ya validada: webhook, HMAC, cliente de WhatsApp, PostgreSQL, migraciones y echo funcional.
    La Fase 2 reemplaza el echo de webhook.rs por un flujo persistente de conversación basado en estado, con menú principal, horarios, programación, recolección de datos y armado
  de pedido hasta ShowSummary, sin entrar todavía a precios, pagos, timers ni lógica del asesor.

    ## Cambios de Implementación

    - Crear el módulo src/bot/ con:
        - mod.rs
        - state_machine.rs
        - states/mod.rs
        - states/menu.rs
        - states/scheduling.rs
        - states/data_collect.rs
        - states/order.rs
        - states/checkout.rs
        - states/advisor.rs
        - states/relay.rs
    - Definir en state_machine.rs:
        - ConversationState con todas las variantes de la spec, aunque en Fase 2 solo se implementen activamente las usadas hasta ShowSummary
        - UserInput
        - BotAction
        - ConversationContext
        - transition(state, input, context) -> Result<(ConversationState, Vec<BotAction>)>
    - Usar conversations.state como string snake_case persistido en DB.
    - Usar conversations.state_data para serializar ConversationContext parcial reutilizando la estructura ya existente en ConversationStateData, ampliándola si hace falta solo
  para
    Fase 2.
    - Integrar webhook.rs con la máquina de estados:
        - extraer UserInput desde IncomingMessage
        - buscar o crear conversación
        - cargar contexto desde DB
        - ejecutar transition
        - ejecutar acciones soportadas
        - persistir nuevo estado y state_data
        - actualizar last_message_at
    - Implementar executor de acciones en webhook.rs o en helper dedicado:
        - SendText
        - SendButtons
        - SendList
        - SendImage
        - ResetConversation
        - NoOp
        - cualquier otra acción futura queda solo definida, no ejecutada todavía

    ## Flujo Funcional a Implementar

    - Estado inicial:
        - cualquier cliente nuevo crea conversación en main_menu
        - cualquier texto libre estando en main_menu muestra bienvenida + lista interactiva del menú principal
    - Menú principal:
        - lista interactiva con 4 opciones:
            - make_order
            - view_menu
            - view_schedule
            - contact_advisor
    - ViewMenu:
        - por ahora usar texto con precios y mensaje de menú; no depender de media aún
        - botones: make_order, back_main_menu
    - ViewSchedule:
        - mensaje con horarios
        - botones: make_order, back_main_menu
    - WhenDelivery:
        - botones immediate_delivery, scheduled_delivery
    - CheckSchedule:
        - usar zona America/Bogota
        - dentro de 8:00 AM a 11:00 PM pasa a CollectName
        - fuera de horario pasa a OutOfHours
    - OutOfHours:
        - botones schedule_later, contact_advisor_now, back_main_menu
        - en Fase 2, contact_advisor_now redirige a ContactAdvisorName solo como navegación, sin relay real
    - Programación:
        - SelectDate recibe texto y valida fecha futura
        - SelectTime recibe texto y valida formato simple reconocible
        - ConfirmSchedule muestra resumen fecha/hora y botones confirm_schedule, change_schedule
    - Recolección:
        - CollectName
        - CollectPhone
        - CollectAddress
        - validar y persistir también columnas customer_name, customer_phone, delivery_address
    - Pedido:
        - SelectType con botones with_liquor, without_liquor
        - SelectFlavor { has_liquor } recibe texto libre normalizado
        - SelectQuantity { has_liquor, flavor } recibe entero positivo 1..999
        - agregar item a context.items
        - AddMore muestra resumen parcial y botones add_more, finish_order
    - Cierre de Fase 2:
        - finish_order transiciona a ShowSummary
        - en Fase 2 ShowSummary solo muestra resumen textual básico sin cálculo de precios ni opciones de pago reales
        - debe dejar explícito que la lógica de precios/pago queda para Fase 3

    ## Decisiones y Defaults

    - No usar imágenes todavía en Fase 2 aunque SendImage exista.
      En ViewMenu y selección de sabores se usa texto temporal.
    - ContactAdvisorName, ContactAdvisorPhone, WaitAdvisorContact, LeaveMessage se definen en el enum pero no se completan funcionalmente en esta fase salvo navegación mínima si
  hace
    falta compilar el match.
    - No implementar timers.
    - No crear órdenes en tabla orders ni order_items todavía.
    - OrderComplete se define pero no se usa aún.
    - Entradas inesperadas en estados ya implementados deben responder con mensaje de corrección y repetir la pregunta actual, no romper el flujo.
    - Si llega un webhook sin messages, se sigue ignorando como hoy.

    ## Pruebas

    - Unit tests para:
        - extract_input con texto, botón, lista e imagen
        - serialización/deserialización de ConversationState persistido como string
        - validaciones de nombre, teléfono, dirección, fecha, hora y cantidad
        - transiciones puras de:
            - MainMenu
            - ViewMenu
            - ViewSchedule
            - WhenDelivery
            - CheckSchedule
            - CollectName
            - CollectPhone
            - CollectAddress
            - SelectType
            - SelectFlavor
            - SelectQuantity
            - AddMore
    - Integración local/manual:
        - cliente escribe hola y recibe menú principal
        - navega a Ver Menú
        - navega a Horarios
        - elige pedido inmediato dentro de horario
        - recolecta nombre, teléfono, dirección
        - agrega al menos dos items mixtos
        - finaliza y llega a ShowSummary
        - reinicio del servidor no pierde el progreso porque el estado está en PostgreSQL

    ## Criterio de Cierre de Fase 2

    - El echo de Fase 1 ya no gobierna el flujo del cliente.
    - La conversación persiste en DB entre mensajes y reinicios.
    - Se puede navegar desde menú principal hasta ShowSummary con datos y items acumulados.
    - El flujo soporta texto, botones y listas según el estado.
    - No hay lógica de pagos, precios, asesor ni timers todavía; esos quedan explícitamente para Fases 3 y 4.

    ## Supuestos

    - Mantendremos el número de prueba de Meta también para Fase 2.
    - No se suben imágenes del menú en esta fase.
    - El texto del menú y sabores puede ser provisional mientras se implementa la máquina de estados.
    - Después de este plan puedes hacer /clear; el implementador solo necesita saber que Fase 1 ya está validada y que el punto de integración principal es src/routes/webhook.rs.