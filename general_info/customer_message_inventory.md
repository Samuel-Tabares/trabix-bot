# Inventario de textos enviados al cliente

Este documento lista los textos que hoy puede enviar el bot al cliente por WhatsApp.

Incluye:

- textos planos
- captions de imagen
- cuerpos de botones y listas
- títulos y descripciones visibles para el cliente
- plantillas dinámicas con variables

No incluye mensajes internos dirigidos solo al asesor.

## 1. Menú principal

Fuente: `src/bot/states/menu.rs`

- Texto:
  - `Hola, bienvenido a Granizados. Elige una opción del menú principal.`
- Lista:
  - cuerpo: `¿Qué deseas hacer?`
  - botón: `Ver opciones`
  - sección: `Menú Principal`
  - opción: `Hacer Pedido` / `Arma tu pedido de granizados`
  - opción: `Ver Menú` / `Sabores y precios`
  - opción: `Horarios` / `Horarios de entrega`
  - opción: `Hablar con Asesor` / `Atención por asesor`
- Reintento:
  - `Selecciona una opción del menú para continuar.`

## 2. Ver menú

Fuente: `src/bot/states/menu.rs`

- Caption de imagen:
  - `Menú general de granizados`
- Texto:
  - `MENÚ Y PRECIOS`
  - `DETAL:`
  - `Con licor: $8.000`
  - `Segundo con licor: $4.000`
  - `Sin licor: $7.000 c/u`
  - `AL MAYOR (20+ del mismo tipo):`
  - `Con licor: 20-49u $4.900 | 50-99u $4.700 | 100+u $4.500`
  - `Sin licor: 20-49u $4.800 | 50-99u $4.500 | 100+u $4.200`
- Botones:
  - cuerpo: `¿Qué deseas hacer ahora?`
  - `Hacer Pedido`
  - `Volver al Menú`

## 3. Ver horarios

Fuente: `src/bot/states/menu.rs`

- Texto:
  - `HORARIOS`
  - `Entrega inmediata: 8:00 AM - 11:00 PM`
  - `Si estás fuera de este horario, aún podemos intentar programar tu pedido o dejarlo listo para asesor.`
- Botones:
  - cuerpo: `¿Deseas hacer un pedido?`
  - `Hacer Pedido`
  - `Volver al Menú`
- Reintento:
  - `Selecciona una opción válida para continuar.`

## 4. Tipo de entrega y programación

Fuente: `src/bot/states/scheduling.rs`

- Botones iniciales:
  - cuerpo: `¿Cuándo lo necesitas?`
  - `Entrega Inmediata`
  - `Entrega Programada`
- Reintento:
  - `Selecciona si tu pedido es inmediato o programado.`
- Fuera de horario:
  - `Estamos fuera del horario de entrega inmediata. Puedes programar tu pedido para después o intentar contactar asesor.`
  - cuerpo botones: `¿Qué deseas hacer?`
  - `Programar`
  - `Asesor`
  - `Menú`
  - reintento: `Selecciona una de las opciones disponibles.`
- Fecha:
  - `Escribe la fecha de entrega como prefieras. Solo necesitamos una referencia para el asesor.`
  - reintento no texto: `Escribe una fecha para programar tu pedido.`
  - validación: `La fecha debe tener entre 2 y 40 caracteres.`
- Hora:
  - `Escribe la hora de entrega como prefieras. Solo necesitamos una referencia para el asesor.`
  - reintento no texto: `Escribe una hora para programar tu pedido.`
  - validación: `La hora debe tener entre 1 y 40 caracteres.`
- Confirmación:
  - `Entrega programada para: {fecha}`
  - `Hora de referencia: {hora}`
  - `¿Confirmas?`
  - cuerpo botones: `Confirma la programación.`
  - `Confirmar`
  - `Cambiar`
  - reintento: `Confirma la programación o elige cambiarla.`

## 5. Captura de datos del cliente

Fuente: `src/bot/states/data_collect.rs`

- Nombre:
  - `¿Nombre completo?`
  - reintento no texto: `Escribe tu nombre completo para continuar.`
  - validación: `El nombre debe tener entre 2 y 80 caracteres.`
- Teléfono:
  - `¿Teléfono de contacto?`
  - reintento no texto: `Escribe un teléfono válido para continuar.`
  - validaciones:
    - `El teléfono debe contener solo dígitos.`
    - `El teléfono debe tener entre 7 y 15 dígitos.`
- Dirección:
  - `¿Dirección de entrega?`
  - reintento no texto: `Escribe la dirección de entrega para continuar.`
  - validación: `La dirección debe tener entre 5 y 160 caracteres.`

## 6. Armado del pedido

Fuente: `src/bot/states/order.rs`

- Tipo de granizado:
  - cuerpo botones: `¿Qué tipo de granizado deseas?`
  - `Con Licor`
  - `Sin Licor`
  - reintento: `Selecciona el tipo de granizado.`
- Selección de sabor:
  - con licor: `Selecciona el sabor con licor que deseas.`
  - sin licor: `Selecciona el sabor sin licor que deseas.`
  - botón lista: `Ver sabores`
  - sección: `Sabores disponibles`
  - descripción común por sabor: `Seleccionar sabor`
- Sabores con licor:
  - `Maracumango Ron blanco`
  - `Blueberry Vodka`
  - `Uva Vodka`
  - `Bonbonbum Whiskey`
  - `Bonbonbum fresa champaña`
  - `Smirnoff de lulo`
  - `Manzana verde Tequila`
- Sabores sin licor:
  - `Maracumango`
  - `Manzana verde`
  - `Bonbonbum`
  - `Blueberry`
- Reintento sabor:
  - `Selecciona un sabor de la lista para continuar.`
- Cantidad:
  - `¿Cuántos de {sabor} con licor deseas?`
  - `¿Cuántos de {sabor} sin licor deseas?`
  - reintento no texto: `Escribe una cantidad válida para continuar.`
  - validaciones:
    - `La cantidad debe ser un número entero.`
    - `La cantidad debe estar entre 1 y 999.`
- Agregar más:
  - `Agregado al pedido.`
  - `Resumen parcial:`
  - cada línea: `- {cantidad} x {sabor} ({con/sin licor})`
  - cuerpo botones: `¿Deseas agregar más granizados?`
  - `Agregar más`
  - `Finalizar pedido`
  - reintento: `Selecciona si deseas agregar más o finalizar el pedido.`

## 7. Resumen y pago

Fuente: `src/bot/states/checkout.rs`

- Resumen dinámico:
  - `RESUMEN DEL PEDIDO`
  - `Cliente: {nombre}`
  - `Teléfono: {telefono}`
  - `Dirección: {direccion}`
  - `Entrega: {tipo}`
  - `Items:`
  - por item: `- {cantidad} x {sabor} ({tipo}, {modo}) -> {subtotal}{promo}`
  - texto promo: ` | promo pares en {n} unidad(es)`
  - `Total estimado: {total}`
  - `Nota: el domicilio no está incluido. Si el pedido es inmediato, un asesor lo define después de revisar tu pedido.`
- Lista de acciones:
  - cuerpo: `Selecciona cómo quieres continuar con tu pedido.`
  - botón lista: `Ver opciones`
  - sección: `Pago y pedido`
  - `Contra Entrega` / `Pagar al recibir el pedido`
  - `Pago Ahora` / `Transferencia y envío de comprobante`
  - `Modificar Pedido` / `Borrar items y volver a elegir sabores`
  - `Cancelar Pedido` / `Salir y volver al menú principal`
- Modificar pedido:
  - `Perfecto. Vamos a reconstruir tu pedido desde los sabores.`
- Comprobante:
  - texto configurable en `config/messages.toml`: `checkout.transfer_payment_text`
  - fallback opcional legado en `.env`: `TRANSFER_PAYMENT_TEXT`
  - `Envía una foto del comprobante en los próximos 10 minutos para continuar.`
  - si envía algo que no es imagen:
    - `Para validar el pago necesito una imagen del comprobante. Envíala como foto por este chat.`
- Timeout comprobante:
  - cuerpo botones: `El tiempo para el comprobante ya venció. Puedes cambiar la forma de pago o cancelar el pedido.`
  - `Cambiar pago`
  - `Cancelar`
- Confirmación de dirección:
  - `Tu dirección de entrega es:`
  - `{direccion}`
  - `¿Es correcta?`
  - `Si, correcta`
  - `Cambiar direccion`
- Cambio de dirección:
  - `Escribe la nueva dirección completa para actualizar el pedido.`
  - si manda algo no texto: `Escribe la nueva dirección en texto para continuar.`
  - si falla validación: primero el mensaje de validación de dirección y luego `Escribe la nueva dirección completa para actualizar el pedido.`

## 8. Espera y negociación con asesor

Fuente: `src/bot/states/advisor.rs`

- Estado de espera general:
  - `Tu pedido ya fue enviado al asesor. Estamos confirmando disponibilidad.`
  - `Tu pedido programado ya fue enviado al asesor. Estamos validando la gestión para la hora acordada.`
  - `Tu pedido al por mayor fue enviado al asesor. Estamos esperando que tome el caso.`
  - `Estamos confirmando disponibilidad con el asesor. Apenas responda te avisamos.`
  - `Estamos cerrando el valor final de tu pedido con el asesor.`
  - `El asesor está revisando una hora para tu pedido.`
  - `Tu propuesta de hora ya fue enviada al asesor. Estamos esperando su respuesta.`
  - `Estamos esperando la confirmación final del asesor.`
  - `El asesor está atendiendo este caso en este momento.`
- Confirmación de hora propuesta por asesor:
  - `El asesor propone entregar {hora}. ¿Aceptas esa hora?`
  - cuerpo botones: `Selecciona cómo deseas continuar.`
  - `Aceptar`
  - `Rechazar`
  - estado repetido:
    - cuerpo botones: `El asesor propone entregar {hora}.`
    - `Aceptar`
    - `Rechazar`
- Contraoferta del cliente:
  - `Escribe la hora que te sirve y se la enviamos al asesor.`
  - si manda algo no texto: `Escribe la hora que te sirve para continuar.`
  - validación: `La hora debe tener entre 1 y 40 caracteres.`
- Confirmación final pedido inmediato:
  - `Pedido confirmado.`
  - `Subtotal: {subtotal}`
  - `Domicilio: {domicilio}`
  - `Total final: {total}`
  - `Dirección: {direccion}`
  - `Tiempo estimado de entrega: 20 a 40 minutos.`
- Confirmación final pedido programado:
  - `Tu pedido fue registrado.`
  - `Fecha de referencia: {fecha}`
  - `Hora acordada: {hora}`
  - `El día de la entrega te contactaremos para gestionar el despacho.`

## 9. Timeout de asesor

Fuente: `src/bot/states/advisor.rs`

- Para pedidos comunes:
  - `El asesor no respondió a tiempo. Puedes reintentar, programar o volver al menú.`
- Para pedidos al por mayor:
  - `El asesor no tomó el caso todavía. Puedes reintentar, programar o volver al menú.`
- Botones:
  - cuerpo: `¿Cómo deseas continuar?`
  - `Programar`
  - `Reintentar`
  - `Menú`

## 10. Hablar con asesor

Fuente: `src/bot/states/advisor.rs`

- Inicio:
  - `¿Nombre del cliente?`
  - `¿Teléfono de contacto?`
- Reintentos:
  - `Escribe tu nombre para continuar.`
  - `Escribe un teléfono válido para continuar.`
- Espera:
  - `Estamos contactando al asesor. Te avisaremos por este chat cuando responda.`
  - `Estamos contactando al asesor. Apenas responda, seguimos por este chat.`
- Si asesor no disponible:
  - cuerpo botones: `El asesor no está disponible en este momento. Puedes dejar un mensaje o volver al menú.`
  - `Dejar mensaje`
  - `Menú`
- Dejar mensaje:
  - `Escribe el mensaje que deseas dejar al asesor.`
  - si manda algo no texto: `Por ahora solo puedo reenviar mensajes de texto al asesor.`
  - validación: `El mensaje debe tener entre 2 y 500 caracteres.`
  - confirmación: `Tu mensaje fue enviado al asesor. Te responderán por ese medio cuando estén disponibles.`

## 11. Relay cliente <-> asesor

Fuentes: `src/bot/states/advisor.rs`, `src/bot/states/relay.rs`

- Cuando asesor toma contacto directo:
  - `El asesor ya está disponible. Puedes escribir por este chat y el mensaje será reenviado.`
- Cuando toma pedido al por mayor:
  - `Tu pedido al por mayor ya quedó conectado con el asesor.`
- Restricción de relay:
  - `En esta fase el relay solo admite mensajes de texto.`
- Cierre:
  - `La conversación con el asesor se cerró por inactividad.`
  - `La conversación con el asesor fue finalizada.`

## 12. Texto configurable fuera del código

Fuente principal: `config/messages.toml`

- `checkout.transfer_payment_text`
  - Este texto se envía al cliente cuando elige `Pago Ahora`.
  - Aquí normalmente van datos bancarios o instrucciones de transferencia.
- Fallback legado:
  - `.env` -> `TRANSFER_PAYMENT_TEXT`
  - Solo se usa si `checkout.transfer_payment_text` está vacío.

## 13. Lugares donde tocar si quieres editar

- `src/bot/states/menu.rs`
- `src/bot/states/scheduling.rs`
- `src/bot/states/data_collect.rs`
- `src/bot/states/order.rs`
- `src/bot/states/checkout.rs`
- `src/bot/states/advisor.rs`
- `src/bot/states/relay.rs`
- `config/messages.toml`

## 14. Observación importante

Los textos del cliente ya quedaron centralizados en `config/messages.toml`. Solo siguen fuera de ese archivo los mensajes internos dirigidos al asesor y el fallback legado de `TRANSFER_PAYMENT_TEXT` en `.env`.
