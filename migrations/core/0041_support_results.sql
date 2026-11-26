ALTER TABLE support_reports
    ADD COLUMN reporter_result text NOT NULL DEFAULT '' CHECK (char_length(reporter_result) <= 32000),
    ADD COLUMN result_key_hash text;
