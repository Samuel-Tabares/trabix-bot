CREATE TABLE customer_addresses (
    id BIGSERIAL PRIMARY KEY,
    customer_phone_meta VARCHAR(20) NOT NULL REFERENCES customers(phone_number_meta),
    address_text VARCHAR(160) NOT NULL,
    address_key VARCHAR(160) NOT NULL,
    zone_kind VARCHAR(20) NOT NULL,
    zone_value VARCHAR(60),
    zone_label VARCHAR(80) NOT NULL,
    last_delivery_cost_cop INT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (customer_phone_meta, address_key)
);

CREATE INDEX idx_customer_addresses_customer ON customer_addresses(customer_phone_meta, created_at DESC);
