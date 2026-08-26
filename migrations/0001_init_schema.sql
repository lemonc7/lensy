CREATE TABLE images (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE
    CHECK (
      length(public_id) = 12
      AND public_id NOT GLOB '*[^0-9A-Za-z]*'
    ),
  status TEXT NOT NULL
    CHECK (
      status IN (
        'uploading',
        'active',
        'trashed',
        'deleting'
      )
    ),
  storage_key TEXT NOT NULL UNIQUE
    CHECK (
      length(storage_key) > 0
      AND storage_key GLOB 'images/*'
    ),
  thumbnail_key TEXT NOT NULL UNIQUE
    CHECK (
      length(thumbnail_key) > 0
      AND thumbnail_key GLOB 'thumbnails/*'
    ),
  original_name TEXT NOT NULL
    CHECK (
      length(trim(original_name)) > 0
      AND length(original_name) <= 255
    ),
  stored_size INTEGER NOT NULL
    CHECK (stored_size > 0),
  thumbnail_size INTEGER NOT NULL
    CHECK (thumbnail_size > 0),
  width INTEGER NOT NULL
    CHECK (width BETWEEN 1 AND 16383),
  height INTEGER NOT NULL
    CHECK (height BETWEEN 1 AND 16383),
  thumbnail_width INTEGER NOT NULL 
    CHECK (thumbnail_width BETWEEN 1 AND 16383),
  thumbnail_height INTEGER NOT NULL 
    CHECK (thumbnail_height BETWEEN 1 AND 16383),
  content_hash TEXT NOT NULL
    CHECK (
      length(content_hash) = 64
      AND content_hash NOT GLOB '*[^0-9a-f]*'
    ),
  pixel_hash TEXT NOT NULL
    CHECK (
      length(pixel_hash) = 64
      AND pixel_hash NOT GLOB '*[^0-9a-f]*'
    ),
  created_at INTEGER NOT NULL
    CHECK (created_at >= 0),
  updated_at INTEGER NOT NULL
    CHECK (updated_at >= 0),
  deleted_at INTEGER
    CHECK (
      deleted_at IS NULL
      OR deleted_at >= 0
    ),

  CHECK (
    (
      status IN ('uploading', 'active')
      AND deleted_at IS NULL
    )
    OR (
      status = 'trashed'
      AND deleted_at IS NOT NULL
    )
    OR status = 'deleting'
  )
) STRICT;

-- 有效图片列表
CREATE INDEX idx_images_active_created
ON images(created_at DESC, id DESC)
WHERE status = 'active';

-- 回收站列表
CREATE INDEX idx_images_trashed_deleted
ON images(deleted_at DESC, id DESC)
WHERE status = 'trashed';

-- 有效图片像素去重
CREATE UNIQUE INDEX idx_images_unique_active_pixel
ON images(pixel_hash)
WHERE status = 'active';

-- 恢复任务
CREATE INDEX idx_image_recovery
ON images(status, updated_at)
WHERE status IN ('uploading', 'deleting');


CREATE TABLE api_tokens (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL 
    CHECK (
      name = trim(name)
      AND length(name) BETWEEN 1 AND 100
    ),
  token_prefix TEXT NOT NULL
    CHECK (
      length(token_prefix) = 12
      AND substr(token_prefix, 1, 6) = 'lensy_'
      AND token_prefix NOT GLOB '*[^0-9A-Za-z_-]*'
    ),
  token_hash TEXT NOT NULL UNIQUE
    CHECK (
      length(token_hash) = 64
      AND token_hash NOT GLOB '*[^0-9a-f]*'
    ),
  created_at INTEGER NOT NULL
    CHECK (created_at >= 0),
  last_used_at INTEGER
    CHECK (
      last_used_at IS NULL
      OR last_used_at >= 0
    ),
  expires_at INTEGER
    CHECK (
      expires_at IS NULL
      OR expires_at > created_at
    ),
  revoked_at INTEGER
    CHECK (
      revoked_at IS NULL
      OR revoked_at >= 0
    )
) STRICT;
