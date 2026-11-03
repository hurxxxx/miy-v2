-- Administrator audit queries filter by actor and period and page by id (V1-A1).
CREATE INDEX audit_actor_id ON audit (actor, id DESC);
CREATE INDEX audit_created_at ON audit (created_at);
