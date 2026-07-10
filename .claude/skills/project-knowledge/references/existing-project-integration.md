# Integración con proyectos existentes

## Qué hacer al entrar a un proyecto por primera vez

1. Buscar carpeta de conocimiento existente (ver orden de búsqueda en `SKILL.md`).
2. Si existe, leer al menos un `SESSION-XXX.md` para detectar:
   - Idioma en uso — seguirlo, no preguntar de nuevo.
   - Nivel de detalle y tono ya establecido — mantener consistencia en vez de imponer el tono por
     defecto de `references/good-practices.md` si el proyecto ya tiene un estilo propio y deliberado.
   - Convención de nombre/carpeta real, aunque difiera de la default de este skill (ver
     `references/edge-cases.md`, caso 3).
3. Si no existe ninguna, es la primera sesión documentada de este proyecto — seguir el default y
   preguntar idioma una sola vez.

## Proyectos con documentación previa no estructurada (README extenso, wiki interna, Notion)

Este skill **no migra** documentación previa a su formato automáticamente. Si el proyecto ya tiene
un README de negocio, una wiki, o páginas de Notion con contexto valioso:
- En Master Mode, tratar esa documentación como fuente de lectura igual que los `SESSION-XXX.md`
  (regla 4 de `SKILL.md`: "toda la documentación del proyecto", no solo las sesiones).
- No dupliques contenido palabra por palabra desde esas fuentes — sintetiza, no copies.
- Si el usuario pide explícitamente "migra las notas de Notion a este formato", eso es un pedido
  puntual y explícito, no un comportamiento automático del skill.

## Roadmap: destinos no-locales (Notion, Confluence, wiki del cliente) — no implementado

Hoy el skill solo escribe archivos Markdown locales (`docs/project-knowledge/`). Una versión futura
podría:
- Detectar si el proyecto centraliza conocimiento en una wiki externa (Notion, Confluence, etc.) y
  ofrecer escribir/actualizar ahí en vez de (o además de) los archivos locales.
- Requerir credenciales/integración MCP específica por herramienta — fuera del alcance actual.

No intentes implementar esto de forma ad-hoc (por ejemplo, llamando a una API de Notion sin que el
usuario lo haya pedido y configurado explícitamente). Si el usuario pregunta por esta capacidad,
responde que está documentada como roadmap futuro, no disponible todavía.

## Cuándo preguntar vs. cuándo decidir solo

| Situación | Acción |
|---|---|
| No hay carpeta de conocimiento y no hay ambigüedad | Decidir solo: crear `docs/project-knowledge/` |
| Hay una carpeta de conocimiento con convención clara | Decidir solo: seguirla |
| Hay más de una carpeta candidata plausible | Preguntar |
| Es la primera sesión documentada del proyecto | Preguntar el idioma, una sola vez |
| El usuario pide explícitamente un destino no soportado (wiki externa) | Explicar que no está implementado todavía |
