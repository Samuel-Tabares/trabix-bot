# Ejemplos

## Ejemplo completo — SESSION-XXX.md (happy path)

Contexto: sesión donde se agregó recuperación de contraseña por email y se optimizó la carga del dashboard.

```markdown
# SESSION-014

## Executive Summary
Agregamos la posibilidad de recuperar la contraseña sin depender de soporte, y el panel principal
ahora carga en menos de un segundo incluso con miles de registros.

## Objectives Achieved
- Recuperación de contraseña autoservicio vía correo electrónico.
- Reducción del tiempo de carga del panel principal.

## Business Problems Solved
- Los usuarios que olvidaban su contraseña dependían de que alguien del equipo se las restableciera
  manualmente, generando tickets de soporte y demoras de horas.
- El panel principal se sentía lento a medida que crecía el volumen de datos, afectando la
  percepción de calidad del producto.

## New Capabilities
- Botón de "olvidé mi contraseña" con verificación por correo.
- Panel principal con tiempos de carga consistentes sin importar el volumen de datos.

## Business Benefits
- Cero tickets de soporte esperados por restablecimiento de contraseña.
- Mejor percepción de velocidad y calidad del producto para usuarios nuevos.

## Before vs After
Antes: un usuario que olvidaba su contraseña escribía a soporte y esperaba horas para recuperar el
acceso. El panel tardaba varios segundos en mostrar información con el volumen actual de datos.
Después: el usuario recupera el acceso en menos de un minuto sin intervención humana. El panel
carga de forma prácticamente instantánea.

## Decisions
Se priorizó la recuperación de contraseña por correo sobre SMS por no requerir costos recurrentes
de un proveedor externo, siendo suficiente para el volumen actual de usuarios.

## Rejected Alternatives
Se evaluó tercerizar todo el inicio de sesión con un proveedor externo de identidad; se descartó por
el costo mensual y porque agregaba una dependencia externa no necesaria en esta etapa.

## Value Generated
Elimina una fuente recurrente de fricción para el usuario final y reduce el trabajo manual del
equipo de soporte a cero en este flujo.

## Features Added
- Recuperación de contraseña por correo.
- Optimización de tiempos de carga del panel principal.

## Future Opportunities
Extender la recuperación de acceso a autenticación por redes sociales cuando el volumen de usuarios
lo justifique.
```

## Ejemplo completo — MASTER_PROJECT_ANALYSIS.md (extracto representativo)

```markdown
# MASTER PROJECT ANALYSIS

## Executive Summary
El proyecto reemplaza un proceso manual de seguimiento de pedidos en hojas de cálculo por una
plataforma centralizada que el equipo del cliente usa a diario para operar el negocio.

## Project History
Arrancó como una herramienta interna simple para reemplazar una hoja de cálculo compartida y creció
hasta cubrir todo el ciclo de vida del pedido, desde su creación hasta la facturación.

## Original Business Problems
El cliente operaba con una hoja de cálculo compartida que se corrompía con frecuencia, no tenía
control de acceso y dependía de una sola persona para consolidar la información cada semana.

## Current Capabilities
Gestión completa de pedidos con acceso simultáneo de todo el equipo, historial de cambios,
recuperación de acceso autoservicio y reportes que antes tomaban un día completo armar manualmente.

## Security
El acceso está restringido por usuario y rol; las contraseñas nunca se almacenan en texto plano; el
acceso se puede revocar de forma inmediata cuando alguien deja el equipo.

## Traceability
Cada cambio de estado de un pedido queda registrado con fecha, hora y usuario responsable,
permitiendo auditar cualquier operación después del hecho.

(...)
```

## Anti-ejemplo (incorrecto) y su corrección

**Incorrecto** — mezcla jerga técnica, referencia código y no traduce a valor de negocio:

```markdown
## Objectives Achieved
- Refactorizamos el AuthController para usar JWT en vez de sessions en Redis.
- Agregamos un índice compuesto en la tabla orders (customer_id, created_at).
- Migramos el pipeline de CI de Jenkins a GitHub Actions.
```

**Por qué falla**: nombra clases, tablas, y herramientas de infraestructura; no dice qué gana el
negocio con ninguno de los tres cambios; un lector no técnico no puede actuar ni decidir nada con
esta información.

**Corregido**:

```markdown
## Objectives Achieved
- El inicio de sesión ahora es más seguro y no depende de un componente adicional para funcionar.
- Las búsquedas de pedidos por cliente y fecha, que antes tardaban varios segundos, ahora son
  instantáneas.
- El proceso de verificación antes de publicar cambios en producción es más rápido y confiable,
  reduciendo el riesgo de interrupciones para los usuarios.
```
