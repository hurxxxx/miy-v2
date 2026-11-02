ALTER TABLE app.assets
    ADD COLUMN location TEXT CHECK (location IS NULL OR char_length(location) <= 100);
