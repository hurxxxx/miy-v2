-- A lifecycle revision is independent of periodic service-health observations.
ALTER TABLE installations ADD COLUMN control_generation bigint NOT NULL DEFAULT 0;
CREATE FUNCTION advance_installation_control_generation() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF ROW(NEW.release_id,NEW.runtime_image,NEW.runtime_container,NEW.runtime_port,
           NEW.desired_state,NEW.runtime_status,NEW.db_status)
       IS DISTINCT FROM
       ROW(OLD.release_id,OLD.runtime_image,OLD.runtime_container,OLD.runtime_port,
           OLD.desired_state,OLD.runtime_status,OLD.db_status) THEN
        NEW.control_generation := OLD.control_generation + 1;
    ELSE
        NEW.control_generation := OLD.control_generation;
    END IF;
    RETURN NEW;
END;
$$;
CREATE TRIGGER installation_control_generation BEFORE UPDATE ON installations
FOR EACH ROW EXECUTE FUNCTION advance_installation_control_generation();

ALTER TABLE runtime_jobs
    ADD COLUMN expected_state jsonb,
    ADD COLUMN expected_checked_at timestamptz,
    ADD COLUMN retry_of uuid REFERENCES runtime_jobs(id),
    ADD COLUMN failure_class text,
    ADD COLUMN retryable boolean NOT NULL DEFAULT false,
    ADD COLUMN request_data jsonb;
