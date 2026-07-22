-- Append-only log of every message that flows through the bot, so the CRM can
-- show the full conversation exactly as it happened -- both the customer<->bot
-- lane and the internal bot<->advisor lane -- for either engine.
--
-- Grouped by `case_phone` (the customer the conversation is about). Advisor
-- messages carry the customer's phone as case_phone even though they are
-- exchanged with the advisor's number, so a case renders as one timeline.
--
--   channel: 'client'  -> the customer<->bot lane
--            'advisor' -> the internal bot<->advisor lane
--   actor:   'client' | 'bot' | 'advisor'  (who produced the message)

CREATE TABLE message_events (
    id            BIGSERIAL PRIMARY KEY,
    case_phone    VARCHAR(20) NOT NULL,
    channel       VARCHAR(10) NOT NULL,
    actor         VARCHAR(10) NOT NULL,
    content_type  VARCHAR(20) NOT NULL,
    body          TEXT,
    payload       JSONB,
    wa_message_id VARCHAR(128),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_message_events_case_created ON message_events (case_phone, created_at);
CREATE INDEX idx_message_events_created ON message_events (created_at DESC);
