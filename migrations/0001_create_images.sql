CREATE TABLE images (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  -- 对外公开的随机id
  public_id TEXT NOT NULL UNIQUE,
  -- 存储路径
  storage_key TEXT NOT NULL UNIQUE,
  thumbnail_key TEXT NOT NULL UNIQUE,
  -- 上传来源信息
  original_name TEXT NOT NULL,
  source_mime TEXT NOT NULL,
  source_size INTEGER NOT NULL CHECK (source_size >= 0),
  -- 最终保存的webp信息
  stored_mime TEXT NOT NULL DEFAULT 'image/webp',
  stored_size INTEGER NOT NULL CHECK (stored_size >= 0),
  thumbnail_size INTEGER NOT NULL CHECK (thumbnail_size >= 0),
  width INTEGER NOT NULL CHECK (width > 0),
  height INTEGER NOT NULL CHECK (height > 0),
  thumbnail_width INTEGER NOT NULL CHECK (thumbnail_width > 0),
  thumbnail_height INTEGER NOT NULL CHECK (thumbnail_height > 0),
  -- 文件哈希
  -- 上传文件，判断原文件是否相同
  source_hash TEXT NOT NULL,
  -- 最终webp文件，用于检查磁盘文件是否损坏
  content_hash TEXT NOT NULL,
  -- 解码像素，判断是否为同一张图片
  pixel_hash TEXT NOT NULL,

  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted_at INTEGER
);

-- 图库游标分页
CREATE INDEX idx_images_active_created
ON images(created_at DESC, id DESC)
WHERE deleted_at IS NULL;

-- 回收站
CREATE INDEX idx_images_deleted
ON images(deleted_at DESC, id DESC)
WHERE deleted_at IS NOT NULL;

-- 按上传文件查重
CREATE INDEX idx_images_source_hash
ON images(source_hash);

-- 按像素查重
CREATE INDEX idx_images_pixel_hash
ON images(pixel_hash);

-- 同一张有效图片只保存一次
CREATE UNIQUE INDEX idx_images_unique_active_pixel
ON images(pixel_hash)
WHERE deleted_at IS NULL;