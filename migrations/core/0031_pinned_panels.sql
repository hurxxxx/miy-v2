-- Side panels a person pinned to the portal header, as "appId/panelId" in
-- display order (docs/APP-FRAMES.md "보조 프레임"). Presentation only.
ALTER TABLE user_preferences
    ADD COLUMN pinned_panels text[] NOT NULL DEFAULT '{}' CHECK (cardinality(pinned_panels) <= 16);
