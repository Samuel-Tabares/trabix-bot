---
name: project-knowledge
description: Preserve project knowledge as business-facing documentation, never code explanations. Session Mode MUST auto-fire the moment the user signals a work session is ending — "cerramos sesión", "cerremos la sesión", "eso es todo por hoy", "terminamos por hoy", "wrap up the session", "that's it for today", "let's close out", "end of session" — producing SESSION-XXX.md without being asked explicitly. If the project is a git repository, Session Mode also commits every pending change from the session (not just the SESSION-XXX.md file) once the doc is written — local commit only, never push. Master Mode reads every SESSION-XXX.md plus project docs to produce MASTER_PROJECT_ANALYSIS.md; it is normally invoked manually via the `/project-knowledge` command, not automatically. Do not trigger for code review, technical documentation (READMEs, API docs), or commit messages.
---

# Project Knowledge

Turn engineering work into documentation a non-technical stakeholder (client, founder, PM) can read to understand business value — never a technical changelog. Two modes, two invocation paths.

## Mode Selection

| Mode | Invocation | Output |
|---|---|---|
| **Session Mode** | Automatic — fires on any session-closing phrase (see description). Do not wait to be asked "document this session". | `SESSION-XXX.md` |
| **Master Mode** | Manual only — the user runs `/project-knowledge`. Do not self-trigger this from conversation, even if the user says "haz el análisis maestro" mid-chat — confirm they want to run it now instead of assuming. | `MASTER_PROJECT_ANALYSIS.md` |

If a message is ambiguous between "log this session" and "give me the master analysis," default to Session Mode — it's the low-cost, high-frequency action; Master Mode is a deliberate, occasional rebuild.

## Reglas estrictas (non-negotiable, both modes)

1. **Nunca código en el output**: cero nombres de función, clases, rutas de archivo, stack traces, nombres de librerías/frameworks. Si una capacidad depende de una librería, descríbela por lo que permite hacer al negocio, no por su nombre técnico.
2. **Toda sección del template debe existir** en el archivo final, incluso si el contenido es "N/A" — nunca borres una sección porque "no aplica esta vez".
3. **Session Mode se basa solo en la sesión actual** (conversación + diffs + commits de ahora). Nunca mezcles contenido de sesiones anteriores — para eso existe Master Mode.
4. **Master Mode lee documentación antes que código**: todos los `SESSION-XXX.md` + docs del proyecto primero; el código fuente es el último recurso, solo para confirmar o rellenar huecos que la documentación no cubre.
5. **Master Mode no genera la documentación adicional que recomienda** — solo la lista en un plan (ver `references/derived-documentation-guide.md`).
6. **Idioma**: por defecto, inglés. Si ya existen `SESSION-XXX.md` previos en el proyecto, sigue el idioma que ya usan (no lo cambies a mitad de proyecto). Si es el primer documento del proyecto y no hay convención previa, pregunta una vez qué idioma usar y no vuelvas a preguntar en sesiones futuras del mismo proyecto.
7. **Alcance de destino actual**: este skill solo escribe Markdown local (ver `references/existing-project-integration.md` para el roadmap de wikis externas — no implementado todavía, no lo intentes).

## Estructura de carpetas y convención de nombres

Buscar, en este orden, antes de crear nada nuevo:

1. `docs/project-knowledge/`
2. `project-knowledge/`
3. Cualquier carpeta que ya contenga `SESSION-*.md` o `MASTER_PROJECT_ANALYSIS.md` (puede tener otro nombre — respétalo, ver `references/existing-project-integration.md`)

Si ninguna existe, crea `docs/project-knowledge/` en la raíz del repo. Si hay más de una carpeta candidata plausible, pregunta antes de escribir.

```text
docs/project-knowledge/
├── SESSION-001.md
├── SESSION-002.md
├── SESSION-003.md
└── MASTER_PROJECT_ANALYSIS.md
```

- Numeración: `SESSION-XXX` con 3 dígitos, cero-padded (`SESSION-001`, no `SESSION-1`).
- `MASTER_PROJECT_ANALYSIS.md` es singular y vive en la misma carpeta — se sobrescribe en cada corrida, no se versiona por fecha.
- No crear subcarpetas por mes/año/feature — todas las sesiones quedan planas en la misma carpeta; el orden lo da el número.

## Detección automática del siguiente SESSION-XXX

1. Listar archivos que matcheen `SESSION-*.md` en la carpeta de conocimiento.
2. Extraer el número de cada nombre (ignorar archivos que no sigan el patrón — ver `references/edge-cases.md` para nombres heredados no numéricos).
3. Tomar el número más alto encontrado y sumar 1. Si no hay ninguno, empezar en `001`.
4. Formatear con cero-padding a 3 dígitos. Si el proyecto ya superó 999 sesiones (no debería pasar), pasa a 4 dígitos manteniendo el padding consistente hacia adelante.
5. Antes de escribir, confirmar que ese número exacto no existe ya (evita condiciones de carrera si se corrió dos veces seguidas) — si existe, incrementa de nuevo.

## Session Mode — pasos

1. Detectar la frase de cierre de sesión (ver descripción) — no esperar confirmación explícita para arrancar.
2. Resolver la carpeta de conocimiento (sección anterior).
3. Calcular el siguiente número (sección anterior).
4. Usar `assets/session-template.md` como estructura de secciones — no inventar ni quitar secciones.
5. Redactar para audiencia no técnica (ver `references/good-practices.md` para tono y nivel de detalle, `references/examples.md` para un ejemplo completo).
6. Guardar como `SESSION-XXX.md`.
7. **Commit automático (si aplica) — ver "Commit al cerrar sesión" abajo.**
8. Confirmar al usuario en una línea: qué se guardó, dónde, y si se hizo commit (y de qué) o por qué no aplicó — no reimprimir el documento completo en el chat salvo que lo pidan.

### Commit al cerrar sesión

Tras guardar `SESSION-XXX.md`, si el proyecto es un repositorio git, commitea **todos** los
cambios pendientes de la sesión — no solo el archivo de sesión — antes de dar por cerrado el
modo. Esto es parte de Session Mode, no un paso opcional.

1. Comprueba si estás dentro de un repositorio git (`git rev-parse --is-inside-work-tree`).
   Si no lo es, no hay nada que hacer aquí — sáltalo sin tratarlo como error.
2. Si es un repo pero no hay ningún cambio pendiente (ni siquiera el `SESSION-XXX.md` recién
   creado, porque ya se guardó fuera del árbol versionado o algo similar), tampoco hay nada
   que commitear — sáltalo.
3. Si hay cambios pendientes: revisa primero si el proyecto tiene sus propias convenciones de
   commit (un `CLAUDE.md`/`AGENTS.md` con una sección de commits, o un skill `commit` local en
   `.claude/skills/`) y síguelas al pie de la letra — tipo de commit, si permite commitear
   directo a `main`/`master` o exige crear rama primero, formato de mensaje, si hay que tocar
   `CHANGELOG.md`. Si el proyecto no define nada propio, usa el skill `commit` genérico de este
   entorno (Conventional Commits, crea rama si estás en `main`/`master` salvo que la convención
   propia del repo diga lo contrario).
4. Agrupa el commit de forma sensata: si el trabajo de la sesión mezcla un cambio de código y
   la documentación, sigue el criterio de granularidad del skill de commit que estés usando
   (normalmente separa código de docs en commits distintos) — no lo fuerces todo a un único
   commit gigante solo por conveniencia, pero tampoco fragmentes artificialmente.
5. **Nunca hagas `git push`, nunca `--force`, nunca `--no-verify`.** El commit queda local; si
   el usuario quiere publicarlo, lo pide aparte — esto es explícitamente fuera del alcance de
   este paso.
6. Si el árbol de trabajo ya tenía cambios sin relación aparente con esta sesión desde ANTES
   de que la sesión empezara, no los mezcles en silencio dentro del mismo commit — coméntalo
   en la confirmación final en vez de asumir que son parte del trabajo de hoy.

## Master Mode — pasos

1. Se invoca vía el comando `/project-knowledge` — no arrancar este modo solo porque el chat lo menciona; confirma primero si no vino del comando.
2. Resolver la carpeta de conocimiento.
3. Leer, en orden, todos los `SESSION-XXX.md` + cualquier documentación Markdown del proyecto (README, specs, decisiones) — antes de tocar código fuente.
4. Reconstruir la narrativa: problema de negocio, capacidades actuales, comparación con el proceso previo del cliente y con software competidor.
5. Solo después de la lectura documental, revisar código para confirmar o extender capacidades no documentadas — nunca como primera fuente.
6. Usar `assets/master-analysis-template.md` como estructura; Seguridad y Trazabilidad son secciones propias, no las mezcles dentro de "Capacidades actuales".
7. En "Future Opportunities", generar el plan de documentación recomendada siguiendo `references/derived-documentation-guide.md` y `assets/documentation-plan-template.md` — listar, no crear esos documentos.
8. Guardar/sobrescribir `MASTER_PROJECT_ANALYSIS.md`.

## Checklist de validación (antes de guardar, ambos modos)

- [ ] Cero identificadores de código (funciones, clases, archivos, stack traces, nombres de librerías).
- [ ] Todas las secciones del template presentes, aunque sea con "N/A".
- [ ] Session Mode: nada de sesiones anteriores se filtró al documento.
- [ ] Master Mode: cada afirmación traza a un `SESSION-XXX.md`, a un doc del proyecto, o está marcada explícitamente como "confirmado leyendo código".
- [ ] Idioma consistente con los documentos previos del proyecto (o confirmado con el usuario si es el primero).
- [ ] Nombre de archivo y carpeta siguen la convención de esta sección — no un esquema improvisado.
- [ ] Si se detectó una convención de carpeta/nombre distinta ya existente en el proyecto, se respetó esa convención en vez de imponer la propia (ver `references/existing-project-integration.md`).
- [ ] Session Mode en un repositorio git: se commiteó (sin `push`) todo el trabajo pendiente de la sesión, siguiendo la convención de commits propia del proyecto si existe; si no era un repo git o no había nada pendiente, no se intentó nada.

## References

| Abrir cuando necesites... | Leer |
|---|---|
| tono, nivel de detalle y redacción para audiencia no técnica | `references/good-practices.md` |
| un ejemplo completo de SESSION y de MASTER, más un anti-ejemplo corregido | `references/examples.md` |
| resolver un caso límite (sesión sin cambios, proyecto sin docs previos, convención de carpeta ya existente, múltiples sesiones el mismo día, revert de trabajo previo) | `references/edge-cases.md` |
| integrarte a un proyecto que ya tiene README/wiki/Notion previo, o entender el roadmap de destinos no-locales | `references/existing-project-integration.md` |
| construir el plan de documentación adicional en Master Mode | `references/derived-documentation-guide.md` |
