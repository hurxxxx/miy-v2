ALTER TABLE release_checks ADD COLUMN checks jsonb NOT NULL DEFAULT '[]'::jsonb CHECK (jsonb_typeof(checks) = 'array');
