-- A third browser purpose for the dev-only app portal. Widen the purpose checks
-- created inline by 0020 (default names <table>_browser_purpose_check). Existing
-- sessions keep their purpose; a portal session simply loses dev app access.
ALTER TABLE sessions DROP CONSTRAINT sessions_browser_purpose_check,
    ADD CONSTRAINT sessions_browser_purpose_check
    CHECK (browser_purpose IN ('portal', 'console', 'dev_portal'));
ALTER TABLE development_sessions DROP CONSTRAINT development_sessions_browser_purpose_check,
    ADD CONSTRAINT development_sessions_browser_purpose_check
    CHECK (browser_purpose IN ('portal', 'console', 'dev_portal'));
ALTER TABLE login_flows DROP CONSTRAINT login_flows_browser_purpose_check,
    ADD CONSTRAINT login_flows_browser_purpose_check
    CHECK (browser_purpose IN ('portal', 'console', 'dev_portal'));
