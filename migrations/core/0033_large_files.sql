-- Large transfer (M10, docs/CORE-PERMISSIONS.md "대용량 전송"): a reservation
-- names its transfer. `single` keeps the 10 MiB one-request upload; `parts`
-- allows up to 2 GiB sent as contiguous ranges, with the committed offset
-- recorded so an interrupted upload resumes from it.
ALTER TABLE app_files DROP CONSTRAINT app_files_size_check;
ALTER TABLE app_files
    ADD COLUMN transfer text NOT NULL DEFAULT 'single' CHECK (transfer IN ('single','parts')),
    ADD COLUMN received bigint NOT NULL DEFAULT 0,
    ADD CONSTRAINT app_files_size_check CHECK (
        size >= 0 AND size <= CASE transfer WHEN 'parts' THEN 2147483648 ELSE 10485760 END
    ),
    ADD CONSTRAINT app_files_received_check CHECK (received BETWEEN 0 AND size);

-- Per-installation limits an administrator sets; absent means the defaults
-- (docs/CAPACITY.md). The whole store is bounded by free disk space instead
-- of a fixed total.
CREATE TABLE app_file_quotas (
    installation_id uuid PRIMARY KEY REFERENCES installations(id) ON DELETE CASCADE,
    max_bytes bigint NOT NULL CHECK (max_bytes BETWEEN 0 AND 1099511627776),
    max_files integer NOT NULL CHECK (max_files BETWEEN 0 AND 10000000),
    updated_by text NOT NULL REFERENCES users(id),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

-- Short-lived download links (M10): issued to the owning app's backend for a
-- file it authorized, bound to the frame that started the request. The frame
-- listener serves the bytes after rechecking the frame, use, approval, the
-- file and the app's own answer for that file.
CREATE TABLE app_file_downloads (
    token_hash text PRIMARY KEY,
    installation_id uuid NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    release_id uuid NOT NULL REFERENCES releases(id),
    file_id uuid NOT NULL REFERENCES app_files(id),
    frame_id uuid NOT NULL REFERENCES app_frame_sessions(id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users(id),
    reference text CHECK (length(reference) BETWEEN 1 AND 512),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX app_file_downloads_expires ON app_file_downloads(expires_at);
