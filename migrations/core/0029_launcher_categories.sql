-- Launcher categories an administrator arranges, ported from AI-DO's
-- platform_app_bar_categories and platform_app_bar_category_apps. An app sits
-- in at most one category; apps in none still appear in the launcher.
-- Presentation only: a category never grants access to an app.
CREATE TABLE launcher_categories (
    id uuid PRIMARY KEY,
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 120),
    icon_key text NOT NULL CHECK (icon_key ~ '^[a-z][a-z0-9-]{0,63}$'),
    position integer NOT NULL CHECK (position >= 0),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    updated_at timestamptz NOT NULL DEFAULT clock_timestamp()
);
CREATE TABLE launcher_category_apps (
    app_id text PRIMARY KEY REFERENCES apps(id) ON DELETE CASCADE,
    category_id uuid NOT NULL REFERENCES launcher_categories(id) ON DELETE CASCADE,
    position integer NOT NULL CHECK (position >= 0)
);
CREATE INDEX launcher_category_apps_category ON launcher_category_apps(category_id, position);
