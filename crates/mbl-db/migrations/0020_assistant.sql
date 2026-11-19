-- Conversations with the built-in assistant, private to each user.
CREATE TABLE assistant_conversations (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE INDEX ix_assistant_conversations_user ON assistant_conversations(user_id, updated_at DESC);

CREATE TABLE assistant_messages (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES assistant_conversations(id) ON DELETE CASCADE,
    role            TEXT NOT NULL CHECK (role IN ('user', 'assistant')),
    content         TEXT NOT NULL,
    created_at      INTEGER NOT NULL
);
CREATE INDEX ix_assistant_messages_conversation ON assistant_messages(conversation_id, id);
