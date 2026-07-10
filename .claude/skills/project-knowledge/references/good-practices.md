# Buenas prácticas de redacción

Guía de tono y nivel de detalle para escribir `SESSION-XXX.md` y `MASTER_PROJECT_ANALYSIS.md`. El lector es quien paga o decide sobre el proyecto, no quien lo programa.

## La prueba del lector

Antes de escribir una frase, pregúntate: "¿esto le importaría a alguien que nunca va a abrir el código?" Si la respuesta es no, no va en el documento.

| Escribiste esto | Reescríbelo como |
|---|---|
| "Refactorizamos el módulo de autenticación para usar JWT en vez de sesiones" | "Hicimos el inicio de sesión más rápido y seguro, sin cambiar cómo lo usan tus usuarios" |
| "Agregamos un índice a la tabla de pedidos" | "Las búsquedas de pedidos que antes tardaban varios segundos ahora son instantáneas" |
| "Migramos de Webpack a Vite" | (probablemente no va en el documento — es una herramienta interna sin impacto de negocio directo; si sí impacta, dilo en términos de "tiempo de build más corto = features llegan más rápido a producción") |

## Nivel de detalle por sección

- **Executive Summary**: 2-4 frases. Si alguien solo lee esto, debe entender qué se ganó.
- **Objectives Achieved / New Capabilities**: listas cortas, una capacidad por bullet, en términos de "ahora puedes...".
- **Business Benefits / Value Generated**: cuantifica cuando se pueda (tiempo ahorrado, errores evitados, capacidad de escalar), pero no inventes números — si no hay dato, describe el beneficio cualitativo.
- **Decisions / Rejected Alternatives**: explica el *por qué* de negocio de una decisión técnica, no el *cómo*. Ejemplo: "Se descartó una integración con [proveedor] por su costo mensual, se optó por una alternativa gratuita con las mismas capacidades para el volumen actual" — sin nombrar el stack técnico exacto usado.

## Errores comunes a evitar

1. **Jerga que se cuela sin querer**: palabras como "endpoint", "deploy", "commit", "bug", "refactor" no significan nada para un cliente no técnico. Tradúcelas: "endpoint" → "función del sistema"; "deploy" → "puesta en producción"; "bug" → "error/falla".
2. **Listar tareas en vez de capacidades**: "Se corrigieron 5 bugs" no dice nada. "El sistema ya no se cae cuando dos usuarios editan al mismo tiempo" sí.
3. **Comparar contra la versión de código anterior en vez del proceso previo del cliente**: "Before vs After" es sobre cómo trabajaba el cliente *antes de tener este software* (Excel, procesos manuales, otro proveedor), no sobre el commit anterior.
4. **Sobre-prometer en "Future Opportunities"**: son oportunidades reales identificadas durante la sesión/proyecto, no una lista de deseos genérica.
5. **Ocultar decisiones incómodas**: si algo se descartó por limitaciones de tiempo o presupuesto, dilo — construye confianza, no lo escondas.

## Tono

- Directo, profesional, sin adornos de marketing ("revolucionario", "de clase mundial").
- Voz activa: "Implementamos X" mejor que "X fue implementado".
- Sin emojis salvo que el cliente los use explícitamente en su propia comunicación.
