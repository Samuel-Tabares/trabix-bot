# Guía para generar documentación derivada (Master Mode)

Master Mode termina con una tabla de documentación recomendada en `Future Opportunities`, usando
`assets/documentation-plan-template.md` como forma. Esta guía define cómo llenarla — Master Mode
**lista** estos documentos, no los escribe.

## Criterio para recomendar un documento

Recomienda un documento cuando falta información que:
1. Un stakeholder necesitaría pedir explícitamente si no existiera (evidencia de una brecha real,
   no una lista de deseos genérica).
2. No está ya cubierta por otro `SESSION-XXX.md`, el propio `MASTER_PROJECT_ANALYSIS.md`, u otra
   documentación existente del proyecto.
3. Tiene un dueño/audiencia identificable — si no sabes quién lo leería, no lo recomiendes todavía.

## Columnas de la tabla

| Columna | Cómo llenarla |
|---|---|
| `Document` | Nombre descriptivo del documento propuesto (ej. "Plan de continuidad ante fallas", "Guía de onboarding para nuevos usuarios") |
| `Audience` | Quién lo va a leer: "Cliente / dueño del negocio", "Nuevo integrante del equipo", "Auditor externo", etc. — nunca "desarrolladores" salvo que el documento sea explícitamente técnico |
| `Priority` | `Alta` / `Media` / `Baja` — ver criterio abajo |
| `Purpose` | Una frase: qué decisión o acción habilita este documento si existiera |

## Criterio de prioridad

- **Alta**: su ausencia ya generó o generará previsiblemente un problema concreto (ej. nadie sabe
  cómo recuperar el sistema ante una falla, y el proyecto ya maneja datos críticos).
- **Media**: mejora la eficiencia o reduce dependencia de una sola persona, pero no hay urgencia
  inmediata.
- **Baja**: nice-to-have, valor incremental, sin costo de no tenerlo a corto plazo.

## Documentos candidatos típicos (no es una lista obligatoria — solo si aplica al proyecto)

- Plan de continuidad / recuperación ante fallas.
- Guía de onboarding para nuevos usuarios o integrantes del equipo.
- Política de acceso y roles.
- Comparativo de costos vs. alternativas del mercado.
- Roadmap de producto a mediano plazo.

## Ejemplo de fila completa

```markdown
| Document | Audience | Priority | Purpose |
|----------|----------|----------|---------|
| Guía de onboarding para nuevos usuarios | Cliente / equipo de soporte del cliente | Media | Reducir el tiempo que toma a un usuario nuevo volverse autónomo en el sistema |
```

## Qué no hacer

- No generar el documento completo "para adelantar trabajo" — el plan es una recomendación, no un
  entregable.
- No recomendar el mismo documento en cada corrida de Master Mode si ya fue marcado como resuelto o
  descartado explícitamente por el usuario en una corrida anterior.
- No inflar la tabla con documentos genéricos de relleno solo para que la sección no se vea vacía —
  una tabla corta y real vale más que una larga y genérica.
