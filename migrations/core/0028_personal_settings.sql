-- Personal settings a person edits in the portal: display settings delivered to
-- app frames, the job title shown with the name, and the launcher pins in order.
-- Presentation only; no permission decision reads this table.
CREATE TABLE user_preferences (
    user_id text PRIMARY KEY REFERENCES users(id),
    theme text NOT NULL DEFAULT 'system' CHECK (theme IN ('system','light','dark')),
    locale text NOT NULL DEFAULT 'ko-KR' CHECK (locale IN ('ko-KR','en-US')),
    time_zone text NOT NULL DEFAULT 'Asia/Seoul' CHECK (char_length(time_zone) BETWEEN 1 AND 64),
    date_format text NOT NULL DEFAULT 'korean' CHECK (date_format IN ('korean','iso','us','european','locale')),
    job_title text CHECK (char_length(job_title) BETWEEN 1 AND 120),
    pinned_apps text[] NOT NULL DEFAULT '{}' CHECK (cardinality(pinned_apps) <= 64),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);

-- A person lists and ends their own browser sessions. The public ID names a
-- session without revealing the cookie hash; the browser line and last use help
-- the person recognize it.
ALTER TABLE sessions
    ADD COLUMN id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    ADD COLUMN user_agent text CHECK (char_length(user_agent) <= 512),
    ADD COLUMN last_seen_at timestamptz;
ALTER TABLE development_sessions
    ADD COLUMN id uuid NOT NULL DEFAULT gen_random_uuid() UNIQUE,
    ADD COLUMN created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    ADD COLUMN user_agent text CHECK (char_length(user_agent) <= 512),
    ADD COLUMN last_seen_at timestamptz;
