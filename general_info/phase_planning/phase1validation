  # Plan de Verificación Completa de Fase 1

  ## Resumen

  Estado actual: la Fase 1 no está verificada por completo todavía.

  Evidencia ya confirmada localmente:

  - cargo build --release compila limpio.
  - cargo test pasa.
  - La lógica de GET /webhook está cubierta por tests.
  - La validación HMAC válida/inválida está cubierta por tests.
  - El parseo de text, button_reply, list_reply e image está cubierto por tests.
  - La serialización de payloads salientes para texto, botones, lista, imagen y mark_as_read está cubierta por tests.

  Puntos aún no verificados operativamente:

  - Arranque real del servidor con DATABASE_URL válida.
  - PostgreSQL real, migraciones reales y CRUD real.
  - Verificación real con Meta.
  - Echo real desde webhook.
  - mark_as_read() real.
  - send_text(), send_buttons() y send_list() contra la API de Meta.
  - Deploy en Railway y visibilidad de logs en Railway.

  ## Cambios Necesarios Para Poder Verificar Todo

  - Agregar una superficie de smoke test en vivo para el cliente de WhatsApp.
    Sin esto, send_buttons() y send_list() no tienen un camino ejecutable desde el flujo actual del bot, porque Fase 1 solo hace echo de texto.
  - La forma recomendada es agregar dos pruebas ignoradas o un binario de smoke test:
      - live_db: corre migraciones y valida CRUD contra una base desechable.
      - live_whatsapp: envía send_text(), send_buttons() y send_list() a un número tester real.
  - Hacer los smoke tests de DB idempotentes o ejecutarlos siempre contra una base vacía.
    Motivo: los tests ignorados actuales usan números fijos y pueden fallar por colisión si se reutiliza la misma base.

  Interfaces nuevas recomendadas para validación:

  - Un smoke test invocable por cargo test --test live_db -- --ignored --test-threads=1.
  - Un smoke test invocable por cargo test --test live_whatsapp -- --ignored --test-threads=1.
  - No cambiar el comportamiento funcional del bot; estas superficies son solo de validación.

  ## Secuencia de Verificación

  1. Preparar infraestructura mínima.

  - Crear un servicio PostgreSQL accesible.
    Recomendado: PostgreSQL en Railway, para usar la misma DATABASE_URL local y en deploy.
  - Tener app de Meta con número de prueba, token, phone_id, verify_token, app_secret y teléfono tester.
  - Cargar localmente DATABASE_URL, WHATSAPP_TOKEN, WHATSAPP_PHONE_ID, WHATSAPP_VERIFY_TOKEN, WHATSAPP_APP_SECRET, ADVISOR_PHONE, y TEST_DATABASE_URL.

  2. Verificación local con recursos reales.

  - Ejecutar cargo build --release.
    Debe seguir sin warnings ni errores.
  - Ejecutar cargo test.
    Debe seguir verde.
  - Ejecutar el smoke test de DB contra una base vacía.
    Debe confirmar migraciones y CRUD básico de conversaciones.
  - Arrancar el servidor con variables reales.
    Debe escuchar en 0.0.0.0:$PORT.
  - Confirmar escucha con una verificación de puerto local.
  - Probar GET /webhook con hub.mode=subscribe, hub.verify_token correcto y hub.challenge.
    Debe devolver el challenge.
  - Probar POST /webhook con firma inválida.
    Debe devolver 401.
  - Probar POST /webhook con firma válida y payload firmado.
    Debe devolver 200.

  3. Verificación funcional de WhatsApp.

  - Usar el smoke test de WhatsApp para enviar al número tester:
      - un texto,
      - un mensaje con botones,
      - un mensaje con lista.
  - Confirmar visualmente en WhatsApp que los 3 mensajes llegaron y que los botones no exceden 3 opciones.
  - Pulsar un botón recibido.
    El webhook desplegado debe parsear button_reply y responder con echo del id.
  - Seleccionar una opción de la lista recibida.
    El webhook desplegado debe parsear list_reply y responder con echo del id.
  - Enviar un texto al número de prueba.
    El bot debe parsearlo y responder con echo.
  - Enviar una imagen al número de prueba.
    El bot debe extraer media_id y responder Recibí tu imagen.
  - En los escenarios de texto, botón, lista e imagen, verificar que el mensaje queda marcado como leído.
    Esto se valida en el flujo real, no como smoke test aislado.

  4. Verificación en Railway.

  - Desplegar la app en Railway con las mismas variables.
  - Asociar PostgreSQL en Railway.
  - Confirmar en logs de arranque que el servicio inicia y ejecuta migraciones sin error.
  - Configurar la URL pública de Railway como webhook en Meta.
  - Completar la verificación de Meta usando GET /webhook.
  - Repetir sobre Railway los escenarios de texto, botón, lista e imagen.
  - Verificar en el dashboard de Railway:
      - logs de arranque,
      - logs de recepción de webhook,
      - logs de envío a Meta,
      - logs de error vacíos o controlados.

  ## Casos de Prueba y Criterio de Cierre

  Checklist solo se considera completo cuando estos puntos queden en estado probado, no inferido:

  - cargo build --release sin warnings ni errores.
  - Servidor arrancando y escuchando en el puerto configurado.
  - GET /webhook verificado por Meta.
  - POST /webhook rechazando firmas inválidas.
  - Echo real para mensajes de texto.
  - Parseo real de button_reply.
  - Parseo real de list_reply.
  - Parseo real de imagen con extracción de media_id.
  - mark_as_read() observable en mensajes reales.
  - send_text() real.
  - send_buttons() real.
  - send_list() real.
  - PostgreSQL real con migraciones ejecutadas.
  - CRUD básico real de conversaciones.
  - Deploy real en Railway.
  - Logs visibles en Railway.

  ## Supuestos

  - Se usará un PostgreSQL desechable o limpio para la validación de DB.
  - Se usará el número de prueba de Meta, no el número productivo.
  - Se añadirá una superficie mínima de smoke test para send_text(), send_buttons() y send_list(), porque el flujo actual de Fase 1 no los dispara por sí solo.
  - Fase 2 no debe empezar hasta cerrar la verificación operativa externa, no solo la cobertura local por tests.