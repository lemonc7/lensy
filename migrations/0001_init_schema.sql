-- 图片是系统的核心业务记录。数据库只保存元数据和相对于 data 目录的
-- 存储键，实际 WebP 文件由文件存储层管理。SQLite 和文件系统无法共享
-- 事务，因此文件操作的一致性由文末的 pending 表与触发器共同保证。
CREATE TABLE images (
  -- AUTOINCREMENT 防止永久删除后复用 ID，使基于 (时间, id) 的游标
  -- 分页顺序长期保持稳定。
  id INTEGER PRIMARY KEY AUTOINCREMENT,

  -- 12 位随机 Base62 标识用于公开 URL，不向外暴露数据库自增 ID。
  public_id TEXT NOT NULL UNIQUE
    CHECK (
      length(public_id) = 12
      AND public_id NOT GLOB '*[^0-9A-Za-z]*'
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
    )
) STRICT;

-- 有效图片按创建时间倒序展示。id 是同一秒内创建多张图片时的稳定
-- 次级排序字段；部分索引不会保存已经进入回收站的记录。
CREATE INDEX idx_images_active_created
ON images(created_at DESC, id DESC)
WHERE deleted_at IS NULL;

-- 回收站按删除时间倒序展示，只索引已经软删除的记录。
CREATE INDEX idx_images_deleted
ON images(deleted_at DESC, id DESC)
WHERE deleted_at IS NOT NULL;

-- 同一份标准化像素只能存在一条有效记录。软删除后允许重新上传相同
-- 图片；恢复时如果已经存在相同图片，唯一约束会阻止产生重复记录。
CREATE UNIQUE INDEX idx_images_unique_active_pixel
ON images(pixel_hash)
WHERE deleted_at IS NULL;

-- API Token 明文只在创建时返回一次，数据库仅保存 SHA-256 哈希。
-- token_prefix 只用于管理页面识别 Token，不参与身份认证。
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

-- 上传文件和写入 images 无法处于同一个事务。上传前先写入本表，文件
-- 落盘并成功插入 images 后由 complete_pending_upload 触发器删除任务。
-- 如果进程中途退出，启动清理可以根据这里保存的路径移除孤儿文件。
CREATE TABLE pending_uploads (
  public_id TEXT PRIMARY KEY
    CHECK (
      length(public_id) = 12
      AND public_id NOT GLOB '*[^0-9A-Za-z]*'
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
  created_at INTEGER NOT NULL
    CHECK (created_at >= 0)
) STRICT;

-- 永久删除 images 记录时先在同一个 SQLite 事务中保存文件清理任务，
-- 数据库提交后再删除磁盘文件。文件删除具有幂等性，失败或进程退出后
-- 可以在下次启动时安全重试。
CREATE TABLE pending_file_deletions (
  storage_key TEXT PRIMARY KEY
    CHECK (
      length(storage_key) > 0
      AND storage_key GLOB 'images/*'
    ),
  thumbnail_key TEXT NOT NULL UNIQUE
    CHECK (
      length(thumbnail_key) > 0
      AND thumbnail_key GLOB 'thumbnails/*'
    ),
  created_at INTEGER NOT NULL
    CHECK (created_at >= 0)
) STRICT;

-- 插入图片和删除 pending upload 发生在同一条 INSERT 语句的事务中，
-- 避免图片记录已经成功但清理标记仍然残留。
CREATE TRIGGER complete_pending_upload
AFTER INSERT ON images
BEGIN
  DELETE FROM pending_uploads
  WHERE public_id = NEW.public_id;
END;

-- 永久删除图片记录时同步保存文件路径。即使随后删除文件时崩溃，
-- 路径也不会随着 images 记录一起丢失。
CREATE TRIGGER enqueue_file_deletion
AFTER DELETE ON images
BEGIN
  INSERT INTO pending_file_deletions (
    storage_key,
    thumbnail_key,
    created_at
  ) VALUES (
    OLD.storage_key,
    OLD.thumbnail_key,
    unixepoch()
  );
END;
