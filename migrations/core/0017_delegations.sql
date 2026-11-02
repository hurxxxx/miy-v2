-- User-to-agent delegation only. Service accounts and recursive delegation are
-- deliberately not modeled by this credential family.
CREATE TABLE delegations (
    id uuid PRIMARY KEY,
    user_id text NOT NULL REFERENCES users,
    parent_session_id uuid REFERENCES cli_sessions,
    name text NOT NULL CHECK(length(name) BETWEEN 1 AND 80),
    app_id text NOT NULL REFERENCES apps,
    environment text NOT NULL CHECK(environment IN ('dev','prod')),
    actions text[] NOT NULL CHECK(cardinality(actions) BETWEEN 1 AND 16),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    CHECK(expires_at>created_at AND expires_at<=created_at+interval '24 hours'),
    CHECK(actions <@ ARRAY['read','app_read','app_write','directory_read','build','upload','dev_session','prepare_database','accept','deploy','promote','start','stop','restart','remove','logs']::text[])
);
CREATE INDEX delegation_owner ON delegations(user_id,id);
ALTER TABLE cli_sessions ADD COLUMN delegation_id uuid REFERENCES delegations;
ALTER TABLE app_call_tokens ADD CONSTRAINT app_call_delegation FOREIGN KEY(delegation_id) REFERENCES delegations;
ALTER TABLE runtime_jobs ADD COLUMN delegation_id uuid REFERENCES delegations;
ALTER TABLE jobs ADD COLUMN delegation_id uuid REFERENCES delegations;
ALTER TABLE app_builds ADD COLUMN delegation_id uuid REFERENCES delegations;
ALTER TABLE image_uploads ADD COLUMN delegation_id uuid REFERENCES delegations;
ALTER TABLE runtime_log_requests ADD COLUMN delegation_id uuid REFERENCES delegations;

CREATE FUNCTION revoke_delegation_credentials() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.revoked_at IS NULL AND NEW.revoked_at IS NOT NULL THEN
        UPDATE cli_sessions SET revoked_at=COALESCE(revoked_at,NEW.revoked_at),
            revocation_reason=COALESCE(revocation_reason,'delegation_revoked')
          WHERE delegation_id=NEW.id;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER delegation_revoke AFTER UPDATE OF revoked_at ON delegations
FOR EACH ROW EXECUTE FUNCTION revoke_delegation_credentials();

-- Re-enabling a user must not revive an earlier delegation.
CREATE FUNCTION revoke_disabled_user_delegations() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.enabled AND NOT NEW.enabled THEN
        UPDATE delegations SET revoked_at=COALESCE(revoked_at,now()) WHERE user_id=NEW.id;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER users_disable_delegations AFTER UPDATE OF enabled ON users
FOR EACH ROW EXECUTE FUNCTION revoke_disabled_user_delegations();

ALTER TABLE jobs ADD COLUMN delegation_action text NOT NULL DEFAULT 'prepare_database';
DROP INDEX app_build_content_identity;
CREATE UNIQUE INDEX app_build_content_identity ON app_builds(actor,app_id,source_commit,archive_hash,platform,COALESCE(delegation_id,'00000000-0000-0000-0000-000000000000'::uuid)) WHERE status<>'failed';
