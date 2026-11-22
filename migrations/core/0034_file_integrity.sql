-- File integrity (M10): the worker recomputes stored SHA-256 sums within a
-- byte budget per run, oldest check first. A mismatch marks the file corrupt;
-- its downloads stop and the platform audits it.
ALTER TABLE app_files
    ADD COLUMN verified_at timestamptz,
    ADD COLUMN corrupt_at timestamptz;
CREATE INDEX app_files_verification ON app_files(verified_at NULLS FIRST, id)
    WHERE status IN ('ready','attached') AND corrupt_at IS NULL;
