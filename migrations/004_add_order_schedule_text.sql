ALTER TABLE orders
ADD COLUMN IF NOT EXISTS scheduled_date_text TEXT,
ADD COLUMN IF NOT EXISTS scheduled_time_text TEXT;

UPDATE orders
SET scheduled_date_text = COALESCE(scheduled_date_text, scheduled_date::TEXT),
    scheduled_time_text = COALESCE(scheduled_time_text, TO_CHAR(scheduled_time, 'HH24:MI:SS'))
WHERE scheduled_date IS NOT NULL
   OR scheduled_time IS NOT NULL;
