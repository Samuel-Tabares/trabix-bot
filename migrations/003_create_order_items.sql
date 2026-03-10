CREATE TABLE order_items (
    id          SERIAL PRIMARY KEY,
    order_id    INT NOT NULL REFERENCES orders(id),
    flavor      VARCHAR(50) NOT NULL,
    has_liquor  BOOLEAN NOT NULL,
    quantity    INT NOT NULL,
    unit_price  INT NOT NULL,
    subtotal    INT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
