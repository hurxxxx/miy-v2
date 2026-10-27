-- Atomic event recording via triggers covers every worker status transition.
CREATE TABLE runtime_job_events (
    job_id uuid NOT NULL REFERENCES runtime_jobs(id) ON DELETE CASCADE,
    sequence bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    payload jsonb NOT NULL,
    PRIMARY KEY(job_id,sequence)
);
CREATE FUNCTION record_runtime_job_event() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP='UPDATE' AND (NEW.status,NEW.step,NEW.error_code) IS NOT DISTINCT FROM (OLD.status,OLD.step,OLD.error_code) THEN RETURN NEW; END IF;
    INSERT INTO runtime_job_events(job_id,sequence,payload)
    SELECT NEW.id,COALESCE(max(sequence),0)+1,jsonb_build_object('id',NEW.id,'appId',NEW.app_id,'environment',NEW.environment,'action',NEW.action,'status',NEW.status,'step',NEW.step,'errorCode',NEW.error_code,'retryable',NEW.retryable,'retryOf',NEW.retry_of)
      FROM runtime_job_events WHERE job_id=NEW.id;
    RETURN NEW;
END $$;
CREATE TRIGGER runtime_job_event AFTER INSERT OR UPDATE ON runtime_jobs FOR EACH ROW EXECUTE FUNCTION record_runtime_job_event();
INSERT INTO runtime_job_events(job_id,sequence,payload)
SELECT id,1,jsonb_build_object('id',id,'appId',app_id,'environment',environment,'action',action,'status',status,'step',step,'errorCode',error_code,'retryable',retryable,'retryOf',retry_of) FROM runtime_jobs;

ALTER TABLE image_uploads ADD COLUMN request_id uuid;
ALTER TABLE image_uploads ADD COLUMN expires_at timestamptz NOT NULL DEFAULT (now()+interval '24 hours');
CREATE UNIQUE INDEX image_upload_request_identity ON image_uploads(actor,request_id) WHERE request_id IS NOT NULL;
