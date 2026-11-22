-- Platform notifications (CORE-PERMISSIONS "플랫폼 알림"): an app backend
-- publishes during a user request; the portal of the same environment shows
-- them. One row per recipient and group key, so a newer notification of the
-- same group (for example a conversation) replaces the older one.
CREATE TABLE notifications (
    id uuid PRIMARY KEY,
    installation_id uuid NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    app_id text NOT NULL,
    environment text NOT NULL CHECK (environment IN ('dev','prod')),
    user_id text NOT NULL REFERENCES users(id),
    kind text NOT NULL CHECK (kind ~ '^[a-z][a-z0-9_.-]{0,39}$'),
    group_key text NOT NULL CHECK (char_length(group_key) BETWEEN 1 AND 200),
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
    path text NOT NULL CHECK (char_length(path) BETWEEN 1 AND 1024),
    resource_id text CHECK (char_length(resource_id) BETWEEN 1 AND 200),
    actor text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    read_at timestamptz,
    UNIQUE (installation_id, user_id, group_key)
);
CREATE INDEX notifications_inbox ON notifications(user_id, environment, created_at DESC, id);
CREATE INDEX notifications_retention ON notifications(created_at);

-- A request ID is accepted once per installation; its result can be read again.
CREATE TABLE notification_requests (
    installation_id uuid NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    request_id uuid NOT NULL,
    actor text NOT NULL,
    input_hash text NOT NULL,
    accepted integer NOT NULL CHECK (accepted >= 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (installation_id, request_id)
);
CREATE INDEX notification_requests_retention ON notification_requests(created_at);

-- Kinds a person turned off per app; '*' turns off every kind of that app.
CREATE TABLE notification_mutes (
    user_id text NOT NULL REFERENCES users(id),
    app_id text NOT NULL CHECK (app_id ~ '^[a-z][a-z0-9-]{0,47}$'),
    kind text NOT NULL CHECK (kind = '*' OR kind ~ '^[a-z][a-z0-9_.-]{0,39}$'),
    PRIMARY KEY (user_id, app_id, kind)
);
