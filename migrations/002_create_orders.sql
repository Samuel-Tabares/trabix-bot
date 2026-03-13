CREATE TABLE orders (
    id                SERIAL PRIMARY KEY,
    conversation_id   INT NOT NULL REFERENCES conversations(id),
    delivery_type     VARCHAR(20) NOT NULL,
    scheduled_date    DATE,
    scheduled_time    TIME,
    scheduled_date_text TEXT,
    scheduled_time_text TEXT,
    payment_method    VARCHAR(20) NOT NULL,
    receipt_media_id  VARCHAR(100),
    delivery_cost     INT,
    total_estimated   INT NOT NULL,
    total_final       INT,
    status            VARCHAR(20) NOT NULL DEFAULT 'pending',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
