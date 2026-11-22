-- Messenger: mentions and reactions (COMMON-FEATURES-PLAN §2.2 "보강"; not in
-- AI-DO 0002af062). See docs/PORTING.md §3.13.

-- One row per mention range of a message. `start_index` and `length` count
-- UTF-16 code units of the stored (trimmed) body, as the browser does; the
-- range starts with "@".
CREATE TABLE dm_message_mentions (
    message_id varchar(36) NOT NULL REFERENCES dm_messages (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    start_index integer NOT NULL CHECK (start_index >= 0),
    length integer NOT NULL CHECK (length BETWEEN 2 AND 300),
    PRIMARY KEY (message_id, start_index)
);
CREATE INDEX ix_dm_message_mentions_user ON dm_message_mentions (user_id, message_id);

-- One row per (message, person, reaction) from a small fixed set.
CREATE TABLE dm_message_reactions (
    message_id varchar(36) NOT NULL REFERENCES dm_messages (id) ON DELETE CASCADE,
    user_id text NOT NULL,
    reaction varchar(16) NOT NULL
        CHECK (reaction IN ('thumbs_up', 'heart', 'laugh', 'surprised', 'sad', 'pray', 'check')),
    created_at timestamp NOT NULL,
    PRIMARY KEY (message_id, user_id, reaction)
);
CREATE INDEX ix_dm_message_reactions_message ON dm_message_reactions (message_id, created_at);
