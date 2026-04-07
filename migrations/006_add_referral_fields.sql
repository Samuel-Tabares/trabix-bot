ALTER TABLE orders
ADD COLUMN referral_code VARCHAR(100),
ADD COLUMN referral_discount_total INT,
ADD COLUMN ambassador_commission_total INT;
