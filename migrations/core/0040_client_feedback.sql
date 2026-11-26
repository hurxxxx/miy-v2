-- Expand anonymous support intake while retaining the existing reports and rate limits.
ALTER TABLE support_reports DROP CONSTRAINT support_reports_payload_check;
ALTER TABLE support_reports ADD CONSTRAINT support_reports_payload_check
    CHECK (octet_length(payload::text) <= 270336);
