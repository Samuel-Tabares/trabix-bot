-- Backfill de `customers` con los clientes anteriores a la tabla.
--
-- `customers` se creó el 2026-07-15 (migración 008) y solo empezó a capturar
-- desde ahí. Los pedidos existen desde marzo: de los 17 teléfonos que han hecho
-- un pedido, solo 4 tenían fila en `customers`. Los otros 13 son invisibles en
-- la consola — no aparecen en contactos, no cuentan para recompra, no existen
-- para ninguna métrica. No es que se hayan perdido: siempre estuvieron en
-- `conversations`/`orders`, nadie los había traído acá.
--
-- Se reconstruye lo que sí se puede derivar de datos ya guardados:
--   - el teléfono y el nombre, de `conversations`
--   - la última dirección de entrega conocida, de `conversations`
--   - primer y último contacto, de las fechas reales de la conversación
--   - gasto y unidades, SOLO de pedidos `confirmed`
--
-- Lo que NO se inventa: `customer_username` y `ctwa_clid` quedan nulos — nunca
-- se capturaron para estos clientes y rellenarlos sería fabricar datos.
--
-- Idempotente: `ON CONFLICT DO NOTHING` sobre la PK. Nunca pisa una fila que el
-- bot ya haya escrito; los 4 clientes existentes quedan intactos.
--
-- Solo entran conversaciones con **evidencia de ser un cliente**: un pedido, un
-- mensaje suyo en la traza, o un nombre/dirección capturados. `conversations`
-- también tiene una fila para el número del ASESOR (el bot la crea para guardar
-- a qué caso está respondiendo), y esa no es un cliente. El filtro la deja
-- afuera sin necesidad de que la migración conozca `ADVISOR_PHONE`, que es
-- config de entorno y acá no se puede leer.

INSERT INTO customers (
    phone_number_meta,
    customer_name_meta,
    delivery_address_last,
    total_spent_cop,
    total_units_purchased,
    first_contact_at,
    last_contact_at
)
SELECT
    cv.phone_number,
    -- Un mismo teléfono puede tener varias conversaciones; se toma el nombre y
    -- la dirección de la más reciente que los tenga.
    (ARRAY_REMOVE(ARRAY_AGG(cv.customer_name ORDER BY cv.created_at DESC), NULL))[1],
    (ARRAY_REMOVE(ARRAY_AGG(cv.delivery_address ORDER BY cv.created_at DESC), NULL))[1],
    COALESCE(paid.spent, 0),
    COALESCE(paid.units, 0),
    MIN(cv.created_at),
    MAX(COALESCE(cv.last_message_at, cv.created_at))
FROM conversations cv
LEFT JOIN LATERAL (
    SELECT COALESCE(SUM(o.total_final), 0)::int AS spent,
           COALESCE(SUM(oi.units), 0)::int      AS units
    FROM orders o
    LEFT JOIN LATERAL (
        SELECT COALESCE(SUM(quantity), 0) AS units
        FROM order_items
        WHERE order_id = o.id
    ) oi ON TRUE
    WHERE o.conversation_id = cv.id
      AND o.status = 'confirmed'
) paid ON TRUE
WHERE EXISTS (SELECT 1 FROM orders o WHERE o.conversation_id = cv.id)
   OR EXISTS (
        SELECT 1 FROM message_events e
        WHERE e.case_phone = cv.phone_number AND e.actor = 'client'
      )
   OR cv.customer_name IS NOT NULL
   OR cv.delivery_address IS NOT NULL
GROUP BY cv.phone_number, paid.spent, paid.units
ON CONFLICT (phone_number_meta) DO NOTHING;
