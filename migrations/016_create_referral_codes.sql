CREATE TABLE referral_codes (
    code VARCHAR(15) PRIMARY KEY,
    active BOOLEAN NOT NULL DEFAULT true,
    boost_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Preserva los codigos legacy de config/referrals.toml como activos, sin boost
-- (el boost pasa a ser una ventana de 7 dias gestionable desde crm-app).
INSERT INTO referral_codes (code, active) VALUES
    ('trabix-prueba15', true),
    ('roma08', true),
    ('jega1', true),
    ('dani2303', true),
    ('dg777', true);
