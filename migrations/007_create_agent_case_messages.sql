CREATE TABLE agent_case_messages (
    phone_number VARCHAR(20) PRIMARY KEY,
    messages     JSONB NOT NULL DEFAULT '[]'::jsonb,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
