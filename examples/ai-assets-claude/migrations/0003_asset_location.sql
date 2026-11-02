ALTER TABLE app.assets
    ADD COLUMN location text
    CHECK (location IS NULL OR (char_length(location) BETWEEN 1 AND 100 AND location = btrim(location)));
