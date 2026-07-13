CREATE TABLE customers (
    phone_number_meta VARCHAR(20) PRIMARY KEY,
    phone_number_manual VARCHAR(20),
    customer_name_meta VARCHAR(80),
    customer_name_manual VARCHAR(80),
    customer_username VARCHAR(50),
    delivery_address_last VARCHAR(160),
    total_spent_cop INT DEFAULT 0,
    total_units_purchased INT DEFAULT 0,
    first_contact_at TIMESTAMPTZ,
    last_contact_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_customers_phone_meta ON customers(phone_number_meta);
CREATE INDEX idx_customers_last_contact ON customers(last_contact_at DESC);
