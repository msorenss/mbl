-- Storage nodes serve real files: a per-node key signs file URLs, and each node library is
-- mirrored as a regular MBL library.
ALTER TABLE remote_nodes ADD COLUMN stream_secret TEXT;

CREATE TABLE remote_node_library_links (
    node_id             INTEGER NOT NULL REFERENCES remote_nodes(id) ON DELETE CASCADE,
    remote_library_id   INTEGER NOT NULL,
    library_id          INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    PRIMARY KEY (node_id, remote_library_id)
);
CREATE UNIQUE INDEX ux_remote_node_library_links_library ON remote_node_library_links(library_id);
