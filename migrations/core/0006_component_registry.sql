-- Component releases are published independently of the CLI and app runtimes.
CREATE TABLE component_releases (
    name text NOT NULL,
    version text NOT NULL,
    digest text NOT NULL,
    package jsonb NOT NULL,
    publisher text NOT NULL,
    published_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (name, version)
);
