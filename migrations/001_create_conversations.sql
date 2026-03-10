CREATE TABLE conversations (
    id               SERIAL PRIMARY KEY,
    phone_number     VARCHAR(20) UNIQUE NOT NULL,
    state            VARCHAR(50) NOT NULL DEFAULT 'main_menu',
    state_data       JSONB DEFAULT '{}',
    customer_name    VARCHAR(100),
    customer_phone   VARCHAR(20),
    delivery_address TEXT,
    last_message_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
