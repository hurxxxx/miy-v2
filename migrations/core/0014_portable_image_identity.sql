-- Docker classic stores address an image by config digest; containerd stores may
-- address the same exported image by a manifest/index digest. Public release and
-- receipt identities stay the client's original immutable image ID.
CREATE TABLE image_imports (
    app_id text NOT NULL REFERENCES apps,
    original_image_id text NOT NULL,
    config_sha256 text NOT NULL,
    platform text NOT NULL,
    source_commit text NOT NULL,
    local_image_id text NOT NULL,
    archive_hash text NOT NULL,
    verified_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (app_id, original_image_id),
    CHECK (original_image_id ~ '^sha256:[0-9a-f]{64}$'),
    CHECK (local_image_id ~ '^sha256:[0-9a-f]{64}$'),
    CHECK (config_sha256 ~ '^[0-9a-f]{64}$'),
    CHECK (archive_hash ~ '^[0-9a-f]{64}$'),
    CHECK (platform IN ('linux/amd64','linux/arm64')),
    CHECK (source_commit ~ '^[0-9a-f]{40}$')
);
ALTER TABLE installations ADD COLUMN runtime_local_image text;
UPDATE installations SET runtime_local_image=runtime_image WHERE runtime_image IS NOT NULL;

CREATE OR REPLACE FUNCTION advance_installation_control_generation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF ROW(NEW.release_id,NEW.runtime_image,NEW.runtime_local_image,NEW.runtime_container,NEW.runtime_port,
           NEW.desired_state,NEW.runtime_status,NEW.db_status)
       IS DISTINCT FROM
       ROW(OLD.release_id,OLD.runtime_image,OLD.runtime_local_image,OLD.runtime_container,OLD.runtime_port,
           OLD.desired_state,OLD.runtime_status,OLD.db_status) THEN
        NEW.control_generation := OLD.control_generation + 1;
    ELSE
        NEW.control_generation := OLD.control_generation;
    END IF;
    RETURN NEW;
END;
$$;
