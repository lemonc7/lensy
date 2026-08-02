CREATE TABLE images (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  storage_key TEXT NOT NULL UNIQUE,
  thumbnail_key TEXT NOT NULL UNIQUE,
  original_name TEXT NOT NULL,
  stored_size INTEGER NOT NULL CHECK (stored_size >= 0),
  thumbnail_size INTEGER NOT NULL CHECK (thumbnail_size >= 0),
  width INTEGER NOT NULL CHECK (width > 0),
  height INTEGER NOT NULL CHECK (height > 0),
  thumbnail_width INTEGER NOT NULL CHECK (thumbnail_width > 0),
  thumbnail_height INTEGER NOT NULL CHECK (thumbnail_height > 0),
  content_hash TEXT NOT NULL,
  pixel_hash TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER
);

CREATE INDEX idx_images_active_created
ON images(created_at DESC, id DESC)
WHERE deleted_at IS NULL;

CREATE INDEX idx_images_deleted
ON images(deleted_at DESC, id DESC)
WHERE deleted_at IS NOT NULL;

CREATE INDEX idx_images_pixel_hash ON images(pixel_hash);

CREATE UNIQUE INDEX idx_images_unique_active_pixel
ON images(pixel_hash)
WHERE deleted_at IS NULL;

CREATE TABLE api_tokens (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL CHECK (length(trim(name)) > 0),
  token_prefix TEXT NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  created_at INTEGER NOT NULL,
  last_used_at INTEGER,
  expires_at INTEGER,
  revoked_at INTEGER
);

CREATE INDEX idx_api_tokens_active
ON api_tokens(expires_at, revoked_at);
