-- Each installed release has an auditable snapshot. NULL means an administrator
-- reviewed its entire immutable manifest (including explicit exclusions).
ALTER TABLE installation_permissions ADD COLUMN reviewed_permissions jsonb
    CHECK (jsonb_typeof(reviewed_permissions) = 'array');
ALTER TABLE installation_permission_history ADD COLUMN inherited_from_release_id uuid REFERENCES releases;

CREATE FUNCTION retained_installation_permissions(previous jsonb, requested jsonb)
RETURNS jsonb LANGUAGE sql IMMUTABLE AS $$
    SELECT coalesce(jsonb_agg(permission ORDER BY permission), '[]'::jsonb)
    FROM jsonb_array_elements_text(previous) AS prior(permission)
    WHERE permission <> 'database' AND requested ? permission
$$;

CREATE FUNCTION carry_installation_consent() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    prior installation_permissions%ROWTYPE;
    target installation_permissions%ROWTYPE;
    requested jsonb;
    reviewed jsonb;
    granted jsonb;
    decider text;
    next_revision bigint;
BEGIN
    SELECT * INTO prior FROM installation_permissions
        WHERE installation_id = NEW.id AND release_id = OLD.release_id;
    SELECT * INTO target FROM installation_permissions
        WHERE installation_id = NEW.id AND release_id = NEW.release_id;
    IF prior.installation_id IS NULL AND target.installation_id IS NULL THEN
        RETURN NEW;
    END IF;

    SELECT manifest->'requestedPermissions' INTO STRICT requested FROM releases
        WHERE id = NEW.release_id AND app_id = NEW.app_id;
    SELECT coalesce(prior.reviewed_permissions, manifest->'requestedPermissions') INTO STRICT reviewed
        FROM releases WHERE id = OLD.release_id AND app_id = NEW.app_id;
    -- On rollback, even an empty current decision supersedes an old target grant.
    IF prior.installation_id IS NULL THEN
        reviewed := '[]'::jsonb;
    END IF;
    reviewed := retained_installation_permissions(reviewed, requested);
    granted := retained_installation_permissions(prior.permissions, requested);
    decider := coalesce(prior.approved_by, target.approved_by);
    INSERT INTO installation_permissions
        (installation_id, release_id, revision, permissions, reviewed_permissions, approved_by, approved_at)
    VALUES (NEW.id, NEW.release_id, 1, granted, reviewed, decider, coalesce(prior.approved_at, target.approved_at))
    ON CONFLICT (installation_id, release_id) DO UPDATE SET
        revision = installation_permissions.revision + 1, permissions = excluded.permissions,
        reviewed_permissions = excluded.reviewed_permissions, approved_by = excluded.approved_by,
        approved_at = excluded.approved_at
    RETURNING revision INTO next_revision;
    INSERT INTO installation_permission_history
        (installation_id, release_id, revision, permissions, actor, inherited_from_release_id)
    VALUES (NEW.id, NEW.release_id, next_revision, granted, decider, OLD.release_id);
    RETURN NEW;
END $$;

CREATE TRIGGER carry_installation_consent AFTER UPDATE OF release_id ON installations
    FOR EACH ROW WHEN (OLD.release_id IS DISTINCT FROM NEW.release_id)
    EXECUTE FUNCTION carry_installation_consent();
