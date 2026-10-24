-- Process state remains in runtime_status; service health is a separate observation.
ALTER TABLE installations
    ADD COLUMN health_status text NOT NULL DEFAULT 'unknown'
        CHECK (health_status IN ('unknown','healthy','degraded','unhealthy','not_applicable')),
    ADD COLUMN health_failures integer NOT NULL DEFAULT 0 CHECK (health_failures >= 0),
    ADD COLUMN health_generation bigint NOT NULL DEFAULT 0,
    ADD COLUMN health_checked_at timestamptz,
    ADD COLUMN health_error text;
