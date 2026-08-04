-- Meta manda el estado real de entrega (sent/delivered/read/failed, con
-- motivo cuando falla) por un webhook de "statuses" separado del mensaje
-- mismo. Hasta ahora se descartaba (solo tracing::debug!, que no queda en
-- producción) — no había forma de saber si un envio que la API aceptó
-- (200 + wa_message_id) de verdad le llegó al cliente. Se enlaza por
-- wa_message_id contra la fila que ya existe en message_events.
ALTER TABLE message_events ADD COLUMN delivery_status VARCHAR(20);
ALTER TABLE message_events ADD COLUMN delivery_status_updated_at TIMESTAMPTZ;
ALTER TABLE message_events ADD COLUMN delivery_error_code BIGINT;
ALTER TABLE message_events ADD COLUMN delivery_error_title VARCHAR(200);

CREATE INDEX idx_message_events_wa_message_id ON message_events (wa_message_id)
    WHERE wa_message_id IS NOT NULL;
