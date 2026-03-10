# Bot WhatsApp Granizados — Documento de Negocio y Operaciones

> Este documento cubre todo lo NO técnico: costos, tiempos, configuración de Meta, riesgos, y plan de desarrollo fase por fase.

---

## 1. Resumen del proyecto

Bot de WhatsApp para automatizar la toma de pedidos de un negocio de granizados en Armenia, Quindío, Colombia. El cliente interactúa con botones, el bot calcula precios automáticamente, y el asesor solo interviene para confirmar pedidos y definir el costo del domicilio. El asesor interactúa con el bot mediante botones enviados a su WhatsApp personal (nunca escribe directamente desde el número del negocio).

---

## 2. Costos operativos mensuales

### 2.1 Desglose

| Concepto | Costo USD | Costo COP (aprox.) | Notas |
|----------|-----------|---------------------|-------|
| Railway Hobby (base) | $5.00 | $21.500 | Incluye $5 de crédito en consumo |
| Railway consumo adicional | ~$1-2 | ~$4.300-8.600 | Bot de bajo tráfico, Rust consume mínimos recursos |
| PostgreSQL en Railway | ~$0.50 | ~$2.150 | Incluido dentro del consumo de Railway |
| WhatsApp Cloud API | ~$0.72 | ~$3.100 | ~180 conversaciones/mes a $0.004 USD cada una |
| Dominio personalizado (opcional) | ~$0.83 | ~$3.575 | $10 USD/año prorrateado. No es necesario, Railway da URL gratis |
| **TOTAL ESTIMADO** | **~$7-9 USD** | **~$30.000-39.000** | **Por mes** |

### 2.2 ¿Cómo escalan los costos?

- **Si el negocio crece a 30 conversaciones/día**: WhatsApp sube a ~$3.60 USD/mes, Railway se mantiene igual. Total: ~$10-12 USD.
- **Si crece a 100 conversaciones/día**: WhatsApp ~$12 USD/mes, Railway podría subir a ~$8-10 USD. Total: ~$20-22 USD.
- **Si crece a 500+/día**: Sería momento de considerar un VPS dedicado (Hetzner ~$4 USD) en vez de Railway para ahorrar. WhatsApp sería el costo principal (~$60 USD/mes).

Los costos solo se vuelven significativos con volúmenes que implicarían un negocio muy exitoso, donde el ingreso justifica sobradamente el gasto.

### 2.3 Costos únicos (setup)

| Concepto | Costo | Notas |
|----------|-------|-------|
| Meta Business verification | Gratis | Pero puede tomar 2-7 días hábiles |
| Número de WhatsApp | Ya lo tienes | Se migra del WhatsApp Business normal a la Cloud API |
| Desarrollo | Tu tiempo | ~8 semanas a medio tiempo (ver plan de desarrollo) |

---

## 3. Configuración de Meta Business y WhatsApp Cloud API

### 3.1 Requisitos previos

- Un número de teléfono con WhatsApp Business (ya lo tienes).
- Una cuenta de Facebook personal (para acceder a Meta for Developers).
- Documentos del negocio para la verificación de Meta Business (RUT, cámara de comercio, o cualquier documento que acredite el negocio).

### 3.2 Pasos de configuración

1. **Crear cuenta en Meta for Developers**: Ir a developers.facebook.com e iniciar sesión con tu cuenta de Facebook.
2. **Crear Meta Business Account**: Ir a business.facebook.com. Si ya tienes una página de Facebook del negocio, posiblemente ya tengas una. Si no, crearla.
3. **Verificar el negocio**: En Business Manager → Configuración → Centro de seguridad → Verificación del negocio. Subir documentos. Esto puede tomar 2-7 días hábiles. Es el paso más lento de todo el proceso.
4. **Crear una App**: En Meta for Developers → Mis Apps → Crear App → Tipo "Business". Asociarla a tu Meta Business Account.
5. **Agregar el producto WhatsApp**: En la app, agregar WhatsApp como producto. Te dará acceso a un número de prueba gratuito para desarrollo.
6. **Registrar tu número real**: En WhatsApp → Configuración de la API → Agregar número de teléfono. **ADVERTENCIA IMPORTANTE**: al registrar tu número en la Cloud API, Meta te pedirá desvincularlo de la app WhatsApp Business normal. Durante la migración, el número queda temporalmente sin servicio (minutos a pocas horas). Planificar esto para un horario de bajo tráfico.
7. **Generar token permanente**: En Business Manager → Configuración → Usuarios del sistema → Crear usuario del sistema → Generar token con permisos `whatsapp_business_management` y `whatsapp_business_messaging`.
8. **Configurar webhook**: En la app de Meta → WhatsApp → Configuración → Webhook. Poner la URL de tu servidor en Railway. Suscribirse a eventos: `messages`.

### 3.3 Número de prueba gratuito

Meta te da un número de prueba con el que puedes desarrollar y probar TODO antes de migrar tu número real. Puedes enviar mensajes a hasta 5 números de prueba (los que tú definas). Esto significa que puedes desarrollar las 8 semanas completas usando el número de prueba y solo migrar tu número real cuando todo esté listo.

### 3.4 Cambio importante: el asesor ya no usa WhatsApp Web con el número del negocio

Al migrar a la Cloud API, la app WhatsApp Business se desvincula del número. El asesor ya no puede abrir WhatsApp Web y escribir desde ese número. En cambio, el asesor recibe las notificaciones y responde a través de botones que el bot le envía a su número personal. Esta es una decisión de diseño, no una limitación: todo queda registrado, el flujo es consistente, y no hay riesgo de que el asesor envíe un mensaje que el bot no espere.

---

## 4. Plan de desarrollo por fases

Estimaciones basadas en dedicación de ~3-4 horas/día, con experiencia previa en proyectos pequeños de Rust.

### Fase 1: Infraestructura base (Semana 1-2)

**Objetivo**: Tener el servidor recibiendo y respondiendo mensajes de WhatsApp.

- Inicializar proyecto Rust con Cargo y dependencias.
- Servidor Axum con verificación de webhook (GET) y recepción de mensajes (POST).
- Validación de firma HMAC-SHA256.
- Structs Serde para payloads de WhatsApp (la parte más laboriosa de esta fase).
- Módulo cliente de WhatsApp: enviar texto, botones, imágenes.
- PostgreSQL con SQLx, migraciones iniciales.
- Dockerfile y primer deploy en Railway.

**Entregable**: El bot recibe un "Hola" del número de prueba, lo loguea, y responde con un mensaje de texto y botones.

### Fase 2: Máquina de estados básica (Semana 3-4)

**Objetivo**: El cliente puede navegar el menú y armar un pedido.

- Enum de estados y función de transición central.
- Menú principal con las 4 opciones.
- Recolección de datos: nombre, teléfono, dirección.
- Selección de granizados: tipo (con/sin licor), sabor (con imagen del menú), cantidad, agregar más.
- Persistencia del estado en PostgreSQL.

**Entregable**: Un cliente puede navegar todo el menú y armar un pedido completo paso a paso.

### Fase 3: Precios, resumen y pago (Semana 5)

**Objetivo**: El pedido se completa con precios correctos y pago.

- Módulo de precios: promo de pares, clasificación detal/mayor, tablas de precios al mayor.
- Resumen del pedido con forma de pago integrada (fusionados en un solo paso).
- Flujo de pago contra entrega y transferencia.
- Timer de espera de comprobante (10 minutos).

**Entregable**: El cliente puede completar un pedido de principio a fin con precios correctos.

### Fase 4: Interacción con asesor (Semana 6-7)

**Objetivo**: El asesor recibe pedidos y responde a través del bot.

- Envío de resumen al asesor con botones (confirmar, no puedo, proponer hora).
- Timer de espera de asesor (2 minutos).
- Paso de confirmación de domicilio (asesor digita valor, bot informa al cliente).
- Negociación de hora (ciclo de propuestas entre asesor y cliente mediado por bot).
- Ruta al mayor → Modo Relay (reenvío de mensajes).
- Opción "Hablar con Asesor" → Modo Relay.
- Timer de inactividad del relay (30 minutos).

**Entregable**: Flujo completo end-to-end funcionando con el asesor.

### Fase 5: Programados, validaciones y pulido (Semana 8)

**Objetivo**: Bot listo para producción.

- Pedidos programados (selección de fecha/hora).
- Horario fuera de servicio.
- Validaciones de input (nombre, teléfono, dirección).
- Manejo del caso de múltiples pedidos pendientes del asesor simultáneamente.
- Testing integral con escenarios reales.
- Migración del número real (ya no usar el de prueba).

**Entregable**: Bot en producción atendiendo clientes reales.

### Resumen de tiempos

| Fase | Duración | Acumulado |
|------|----------|-----------|
| Fase 1: Infraestructura | 2 semanas | 2 semanas |
| Fase 2: Máquina de estados | 2 semanas | 4 semanas |
| Fase 3: Precios y pago | 1 semana | 5 semanas |
| Fase 4: Asesor y relay | 2 semanas | 7 semanas |
| Fase 5: Pulido y lanzamiento | 1 semana | **8 semanas** |

---

## 5. Mejoras futuras (no necesarias para el lanzamiento)

Estas funcionalidades agregan valor a medida que el negocio escala. Se implementan después de que el bot esté en producción y estable.

- **Clientes recurrentes**: Identificar al cliente por número de WhatsApp, precargar nombre/teléfono/dirección favorita. Reduce fricción de recompra. Requiere nueva tabla `customers` en la base de datos y un paso nuevo al inicio del flujo ("¿Quieres pedir a la misma dirección de siempre?").
- **Timer de abandono de conversación**: Si el cliente deja de responder durante la recolección de datos (15 min), limpiar el estado y enviar mensaje de retoma. Evita conversaciones "fantasma" acumulándose.
- **Estadísticas**: Sabores más vendidos, horarios pico, ticket promedio, tasa de conversión, tasa de timeouts del asesor.
- **Notificaciones proactivas**: Enviar promos a clientes frecuentes usando Message Templates de WhatsApp (requiere aprobación previa de Meta para cada template).
- **Múltiples asesores**: Distribuir pedidos entre varios asesores según disponibilidad. Útil si el negocio crece y un solo asesor no da abasto.
- **Verificación automática de pagos**: Integración con Nequi o Daviplata para confirmar pagos automáticamente sin que el asesor revise comprobantes.
- **Panel web**: Si el volumen crece mucho, un dashboard para que el asesor vea todos los pedidos en tiempo real sin depender de WhatsApp.

---

## 6. Riesgos y mitigaciones

| Riesgo | Impacto | Mitigación |
|--------|---------|------------|
| Meta rechaza o demora la verificación del negocio | Bloquea el uso del número real | Iniciar el trámite en paralelo con la Fase 1. Desarrollar con el número de prueba gratuito de Meta. |
| La curva de Rust toma más de lo estimado | Retraso en Fases 1-2 | Los structs de WhatsApp son lo más complejo. Dedicar tiempo extra ahí; el resto fluye más rápido. |
| Railway cambia pricing o tiene downtime | Aumento de costos o bot caído | El binario de Rust es portable. Migrar a Fly.io o un VPS toma menos de 1 hora. |
| Meta cambia la API de WhatsApp | Bot deja de funcionar | Meta anuncia deprecaciones con meses de anticipación. El módulo `whatsapp/client.rs` está aislado para facilitar actualizaciones. |
| El asesor no responde a tiempo sistemáticamente | Mala experiencia del cliente | Ya manejado: timeouts con opciones de programar o reintentar. Monitorear tasa de timeouts para ajustar el proceso. |
| El asesor se confunde con el nuevo sistema (botones en vez de escribir directo) | Pedidos mal atendidos al inicio | Hacer pruebas con el asesor usando el número de prueba antes de migrar. Darle una semana de práctica. |
| Desvinculación del número de WhatsApp Business falla o toma mucho tiempo | Número sin servicio durante horas | Programar la migración para un domingo a la noche o un horario de mínimo tráfico. |

---

## 7. Checklist de lanzamiento

Antes de poner el bot en producción con clientes reales:

- [ ] Meta Business verificado y aprobado.
- [ ] App de Meta creada con producto WhatsApp configurado.
- [ ] Bot desplegado en Railway y respondiendo correctamente con el número de prueba.
- [ ] Todos los flujos testeados (detal, mayor, programado, hablar con asesor, fuera de horario, timeouts).
- [ ] El asesor practicó con el sistema usando el número de prueba por al menos una semana.
- [ ] Imágenes del menú de sabores subidas y configuradas.
- [ ] Variables de entorno de producción configuradas en Railway.
- [ ] Número real migrado a la Cloud API.
- [ ] Webhook apuntando a la URL de producción en Railway.
- [ ] Primer pedido real de prueba completado exitosamente.