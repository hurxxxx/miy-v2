CREATE TABLE official_apps (
    app_id text PRIMARY KEY REFERENCES apps(id),
    default_install boolean NOT NULL
);

CREATE TABLE official_app_versions (
    app_id text NOT NULL REFERENCES official_apps(app_id),
    version text NOT NULL,
    source_commit text NOT NULL CHECK (source_commit ~ '^[0-9a-f]{40}$'),
    archive_sha256 text NOT NULL CHECK (archive_sha256 ~ '^[0-9a-f]{64}$'),
    PRIMARY KEY (app_id, version)
);

-- Installed releases continue to identify the exact publisher's source even
-- after the operator imports newer catalog versions.
CREATE FUNCTION keep_official_app_version() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'official_version_immutable';
END;
$$;
CREATE TRIGGER official_app_version_immutable BEFORE UPDATE OR DELETE ON official_app_versions
    FOR EACH ROW EXECUTE FUNCTION keep_official_app_version();

ALTER TABLE releases
    ADD COLUMN official_version text,
    ADD CONSTRAINT release_official_version_matches CHECK (
        official_version IS NULL OR
        (official_version = version AND source_commit IS NOT NULL AND image_id IS NOT NULL)
    ),
    ADD FOREIGN KEY (app_id, official_version) REFERENCES official_app_versions(app_id, version);

CREATE INDEX app_build_succeeded_image ON app_builds(app_id, image_id) WHERE status='succeeded';

-- A legacy worker omits official_version when creating a release. Protect
-- registered products even if that worker runs against the upgraded schema.
CREATE FUNCTION check_official_release() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' AND OLD.official_version IS NOT NULL THEN
        IF NEW IS DISTINCT FROM OLD THEN
            RAISE EXCEPTION 'official_release_immutable' USING ERRCODE='check_violation';
        END IF;
        RETURN NEW;
    END IF;
    IF EXISTS (SELECT 1 FROM official_apps WHERE app_id=NEW.app_id)
       AND NOT EXISTS (
            SELECT 1 FROM official_app_versions v
            JOIN app_builds b ON b.app_id=v.app_id AND b.version=v.version
                AND b.source_commit=v.source_commit AND b.archive_hash=v.archive_sha256
            WHERE v.app_id=NEW.app_id AND v.version=NEW.official_version
                AND b.version=NEW.version AND b.source_commit=NEW.source_commit
                AND b.image_id=NEW.image_id AND b.status='succeeded'
       ) THEN
        RAISE EXCEPTION 'official_release_required' USING ERRCODE='check_violation';
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER official_release_provenance BEFORE INSERT OR UPDATE ON releases
    FOR EACH ROW EXECUTE FUNCTION check_official_release();
