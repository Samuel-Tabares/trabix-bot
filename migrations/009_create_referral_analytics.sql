CREATE TABLE referral_code_analytics (
    code VARCHAR(15) PRIMARY KEY,
    times_used INT DEFAULT 0,
    total_discount_generated_cop INT DEFAULT 0,
    total_commission_generated_cop INT DEFAULT 0,
    total_units_purchased INT DEFAULT 0,
    total_sales_cop INT DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_referral_code ON referral_code_analytics(code);
CREATE INDEX idx_referral_updated ON referral_code_analytics(updated_at DESC);
