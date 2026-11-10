-- Migration 0012: Ensure non-clone avatars can be listed and cloned in the Souk
UPDATE avatars
SET listed = true, price_yocto = 500000000000000000000000
WHERE cloned_from IS NULL AND (price_yocto IS NULL OR price_yocto = 0);
