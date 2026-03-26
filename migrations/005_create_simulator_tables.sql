CREATE TABLE simulator_sessions (
    id             SERIAL PRIMARY KEY,
    customer_phone VARCHAR(20) UNIQUE NOT NULL,
    profile_name   VARCHAR(100),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE simulator_messages (
    id           BIGSERIAL PRIMARY KEY,
    session_id   INT REFERENCES simulator_sessions(id) ON DELETE CASCADE,
    actor        VARCHAR(20) NOT NULL,
    audience     VARCHAR(20) NOT NULL,
    message_kind VARCHAR(20) NOT NULL,
    body         TEXT,
    payload      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX simulator_messages_session_id_idx
    ON simulator_messages(session_id, id);

CREATE INDEX simulator_messages_audience_idx
    ON simulator_messages(audience, id);

CREATE TABLE simulator_media (
    id                VARCHAR(100) PRIMARY KEY,
    session_id        INT REFERENCES simulator_sessions(id) ON DELETE CASCADE,
    kind              VARCHAR(30) NOT NULL,
    file_path         TEXT NOT NULL,
    mime_type         VARCHAR(100),
    original_filename TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
