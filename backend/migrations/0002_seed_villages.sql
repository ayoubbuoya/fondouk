-- The 4 launch villages (D14). Pool ids come from DefiLlama (HANDOFF U1).
-- map_x / map_y are placeholders on a simple 2x2 grid until the Tiled map exists (Day 6).
INSERT INTO villages (id, name, kind, protocol, asset, pool_ref, map_x, map_y) VALUES
    ('rhea-lend-usdc',     'Rhea Lend USDC',   'lending',   'rhea', 'USDC',
        '1a54c607-d829-5621-87a7-e3b7debc51a1', 100, 100),
    ('rhea-lend-usdt',     'Rhea Lend USDT',   'lending',   'rhea', 'USDT',
        'd4c96d26-5869-5771-90d5-08fd54e1455e', 300, 100),
    ('rhea-dex-stable4',   'Rhea Stable Pool', 'stable_lp', 'rhea', 'USDT-USDC-USDT.e-USDC.e',
        '2fc37959-7f8b-5016-a18d-902ab72b0d4b', 100, 300),
    ('rhea-dex-frax-usdc', 'Rhea FRAX-USDC',   'stable_lp', 'rhea', 'FRAX-USDC',
        '94a8d511-c96e-5503-a81d-3357877d9c8a', 300, 300);
