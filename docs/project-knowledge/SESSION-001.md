# SESSION-001

## Executive Summary

Construimos y probamos en vivo la base de una versión con inteligencia artificial del asistente de pedidos por WhatsApp: ahora puede sostener una conversación natural con el cliente y coordinarse directamente con el asesor humano, en lugar de depender únicamente de botones fijos, sin perder la exactitud de precios y reglas de negocio del sistema actual.

## Objectives Achieved

- Los clientes ya pueden escribir de forma natural (como si le escribieran a una persona) para ver el menú, preguntar el horario, armar un pedido y pagar, en vez de solo poder tocar botones.
- El costo de domicilio se calcula automáticamente para Armenia (por zona) y para 14 municipios cercanos, sin que el asesor tenga que escribir el valor a mano.
- Para esos destinos, el rol del asesor se simplifica a confirmar con un simple "sí/no" si puede atender el pedido, en vez de calcular precios y gestionar todo manualmente.
- La elección del método de pago, la confirmación del comprobante de transferencia, y los códigos de descuento de embajadores también funcionan ahora de forma conversacional.
- Se validó en vivo, con escenarios donde el dinero está de por medio, que los totales y los pedidos registrados coinciden exactamente con lo que produciría el sistema actual.

## Business Problems Solved

- El asistente actual solo entiende toques sobre botones fijos: un cliente que escribe naturalmente, hace una pregunta a mitad de su pedido, o quiere pedir todo en un solo mensaje, tenía que seguir sí o sí el guion paso a paso.
- El asesor tenía que calcular y escribir manualmente el costo de domicilio en cada pedido, incluso para destinos conocidos donde el valor nunca cambia.
- Durante las pruebas se descubrió un riesgo real: si el asesor y un cliente actuaban sobre el mismo pedido casi al mismo tiempo, una versión temprana del sistema podía perder silenciosamente parte de la información del pedido. Se encontró y se corrigió antes de que representara un riesgo real para el negocio.

## New Capabilities

- Conversación libre para hacer pedidos: saludo, ver menú, preguntar horario, armar un pedido con varios productos, y confirmar — todo en lenguaje natural.
- Cálculo automático de domicilio por zona de Armenia (norte/centro/sur) y por 14 municipios cercanos, cada uno con su tarifa ya acordada.
- Coordinación con el asesor humano manejada por el mismo asistente: prepara el resumen del pedido, le pide al asesor una confirmación simple, y le transmite los datos de pago — reduciendo la digitación manual del asesor en destinos conocidos.
- Reconocimiento automático de clientes que ya han pedido antes (nombre, teléfono, dirección) sin necesidad de volver a preguntar.
- Existe un interruptor de seguridad para apagar instantáneamente este modo conversacional y volver al sistema actual, ya probado, sin necesidad de cambios adicionales.

## Business Benefits

- Los clientes que prefieren escribir todo su pedido en un solo mensaje, en vez de navegar varios menús, ahora pueden ser atendidos sin fricción.
- El cálculo de domicilio para los destinos más frecuentes (Armenia + 14 municipios cercanos) ahora es instantáneo y sin margen de error, eliminando un paso manual de cada pedido a esos destinos.
- Reduce el trabajo del asesor en pedidos de rutina a una simple confirmación de sí/no, liberando su tiempo para los casos que sí requieren su criterio (destinos poco comunes, pedidos pequeños fuera de la zona de cobertura).
- El sistema se puso a prueba contra un escenario realista de alto riesgo — el asesor y el cliente actuando sobre el mismo pedido casi al mismo tiempo — y ahora garantiza que no se pierde información del pedido en ese caso, aunque sea una situación poco frecuente.

## Before vs After

**Antes**: todo pedido, sin importar el destino, requería que el asesor escribiera manualmente el valor del domicilio y que la conversación avanzara únicamente a través de botones fijos; los clientes no podían hacer un pedido o una pregunta en texto libre sin seguir exactamente la secuencia de toques definida.

**Después**: para pedidos dentro de Armenia o hacia uno de los 14 municipios cercanos ya aprobados, el costo de domicilio es automático y el asesor solo confirma disponibilidad; los clientes pueden pedir, preguntar y pagar conversando con naturalidad, de forma similar a como lo harían con una persona real, manteniendo exactamente los mismos precios, descuentos y registros de pedido que el sistema actual.

## Decisions

- Se decidió que todo el cálculo de precios, reglas de descuento y lógica de negocio siga corriendo en el mismo sistema confiable y ya probado de hoy — la capa conversacional solo decide *qué* preguntar o decir, nunca *cuánto* cuesta algo. Fue una decisión deliberada para evitar el riesgo de que la inteligencia artificial invente o calcule mal un precio.
- Se decidió que los mensajes automáticos de recordatorio o vencimiento de tiempo (por ejemplo, "seguimos esperando al asesor") sigan funcionando exactamente igual que hoy, sin involucrar inteligencia artificial en esos casos — esto mantiene bajo el costo operativo, ya que son mensajes frecuentes y repetitivos que no requieren criterio.
- Se decidió que las acciones se ordenen una por una únicamente dentro de un mismo pedido (para que un mensaje del asesor y uno del cliente sobre el MISMO pedido nunca se crucen), mientras que pedidos de clientes distintos siguen siendo completamente independientes y simultáneos — esto responde directamente a la necesidad del negocio de atender varios pedidos al mismo tiempo.
- Para destinos fuera de Armenia y de los 14 municipios cercanos conocidos, se mantuvo el proceso totalmente manual (el asesor da el precio) y se exige una cantidad mínima de pedido, igual que hoy se manejan los envíos más grandes o menos frecuentes.
- El nuevo modo conversacional está restringido por ahora a un entorno interno de pruebas — no puede activarse por accidente para clientes reales hasta que se habilite explícitamente en producción.

## Rejected Alternatives

- Se consideró que la inteligencia artificial solo interpretara los mensajes en texto libre de los clientes, dejando el lado del asesor exactamente como está hoy (solo botones fijos, sin IA). Se descartó a favor de una solución más completa donde el mismo asistente también coordina con el asesor, según lo solicitado explícitamente, luego de confirmar que esto podía hacerse sin debilitar la seguridad del cálculo de precios ni de las confirmaciones del asesor.
- Se consideró que el sistema adivinara automáticamente la zona de entrega de un cliente a partir de su dirección. Se descartó por el riesgo económico de asignar la zona equivocada y por lo tanto el precio equivocado; el asistente ahora simplemente le confirma la zona directamente al cliente.

## Value Generated

Elimina un paso manual repetitivo (escribir el costo de domicilio) en los destinos de pedido más frecuentes, acorta el tiempo entre que un cliente confirma su pedido y el asesor puede responder, y le da a los clientes una experiencia de pedido más natural sin sacrificar la exactitud de precios, descuentos y registros de pedidos de los que depende el negocio hoy.

## Features Added

- Asistente conversacional de pedidos para clientes (menú, horario, armar pedido, pago).
- Cálculo automático de domicilio para Armenia (por zona) y 14 municipios cercanos.
- Coordinación conversacional entre el asistente y el asesor humano para disponibilidad, pago y comprobantes.
- Reconocimiento automático de datos guardados de clientes que ya han pedido antes.
- Interruptor de seguridad para desactivar el modo conversacional al instante si hiciera falta.

## Future Opportunities

- Evaluar un modelo de inteligencia artificial más avanzado si el actual llega a omitir datos en mensajes de clientes muy largos y con mucha información junta.
- Extender el asistente conversacional a los escenarios que hoy siguen siendo manuales: reprogramar cuando el asesor no puede atender de inmediato, tomar pedidos grandes al por mayor, y solicitudes generales de "hablar con un asesor" sin un pedido asociado.
- Agregar pruebas automatizadas para los nuevos flujos conversacionales, de forma que cualquier problema se detecte antes de llegar a clientes reales.
- Definir un plan de lanzamiento gradual (por ejemplo, empezar con un número reducido de clientes reales) antes de activar el modo conversacional en producción.
