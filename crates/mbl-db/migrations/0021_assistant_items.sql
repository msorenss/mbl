-- Titles from the library that an assistant answer mentions, as JSON, for links in the chat.
ALTER TABLE assistant_messages ADD COLUMN items TEXT;
