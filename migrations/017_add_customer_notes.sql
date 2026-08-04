-- Memoria semántica y barata del agente sobre un cliente: una nota corta,
-- en lenguaje natural, que el propio modelo escribe/actualiza cuando
-- aprende algo que vale la pena recordar de una conversación a otra (cómo
-- le gusta que le hablen, preferencias recurrentes) — no es el transcript
-- crudo (eso es `agent_case_messages`, que sí se limpia tras cada checkout),
-- así que leerla en cada turno es barato.
ALTER TABLE customers ADD COLUMN customer_notes VARCHAR(300);
