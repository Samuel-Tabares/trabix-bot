# Casos límite

## 1. Sesión sin cambios de código (investigación, debugging sin fix, exploración)

No todas las sesiones cierran con una feature. Sigue generando `SESSION-XXX.md` — el valor de negocio
puede ser "descartamos una opción costosa" o "confirmamos la causa de un problema, la solución viene
la próxima sesión". Usa:
- `Objectives Achieved`: lo que se investigó/descartó, no una feature inexistente.
- `New Capabilities`: "N/A — sesión de investigación, sin cambios visibles para el usuario todavía".
- `Decisions` / `Rejected Alternatives`: aquí vive la mayor parte del valor de esta sesión.

No inventes una capacidad para rellenar la sección.

## 2. Proyecto sin documentación previa (primera sesión documentada)

No hay `SESSION-XXX.md` previos ni convención de carpeta/idioma establecida.
- Aplica el árbol de carpetas por defecto (`docs/project-knowledge/`).
- Pregunta el idioma una sola vez (ver regla 6 en `SKILL.md`) y no vuelvas a preguntar en sesiones
  futuras del mismo proyecto.
- Empieza en `SESSION-001`.
- Si el proyecto ya tiene meses de historia sin documentar, no intentes reconstruir sesiones pasadas
  retroactivamente — arranca desde la sesión actual. Si el usuario pide reconstruir el pasado,
  eso es un `MASTER_PROJECT_ANALYSIS.md` construido desde código/docs existentes, no sesiones
  inventadas.

## 3. El proyecto ya tiene una convención de carpeta/nombre distinta

Ejemplo: ya existe `notes/weekly/2026-07-01.md` en vez de `SESSION-XXX.md`.
- No impongas la convención de este skill sobre una preexistente y en uso activo.
- Sigue el patrón de nombre/ubicación ya establecido, adaptando el *contenido* (secciones del
  template, tono no técnico) sin romper el *esquema de archivos* del proyecto.
- Si la convención existente es ambigua o mezclada (algunos archivos siguen un patrón, otros otro),
  pregunta antes de decidir cuál seguir — no promedies dos convenciones.
- Ver `references/existing-project-integration.md` para más detalle.

## 4. Múltiples sesiones el mismo día

Cada sesión de trabajo (cada vez que se dispara la frase de cierre) es un archivo nuevo, sin
importar si ya existe uno con la fecha de hoy. La numeración es secuencial por sesión, no por día:
`SESSION-014` y `SESSION-015` pueden ser el mismo día. No fusiones dos sesiones en un solo archivo
aunque el tema se parezca — son unidades de trabajo distintas y el cliente puede querer ver el
progreso incremental.

## 5. La sesión revierte o deshace trabajo de una sesión anterior

Documenta el revert como su propia sesión, con su propia numeración — no reescribas ni elimines el
`SESSION-XXX.md` de la sesión original (es historial, no se edita retroactivamente). En la nueva
sesión:
- `Executive Summary`: explica qué se revirtió y por qué, en términos de negocio ("la función X
  causaba un problema con Y, se desactivó temporalmente mientras se ajusta el enfoque").
- `Decisions`: el motivo de negocio del revert (no "el código rompía en producción" en términos
  técnicos, sino el impacto real: "afectaba la disponibilidad del sistema para los usuarios").
- No trates el revert como algo vergonzoso a ocultar — es información legítima para el cliente.

## Detección de numeración con nombres heredados no numéricos

Si la carpeta de conocimiento tiene archivos que no siguen `SESSION-XXX.md` (por ejemplo, notas
sueltas o el `notes/weekly/...` del caso 3), ignóralos para efectos de calcular el siguiente número,
pero no los borres ni los muevas — no son responsabilidad de este skill a menos que el usuario pida
explícitamente migrarlos.
