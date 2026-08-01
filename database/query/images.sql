-- name: CreateImage :one
INSERT INTO images (
  public_id, storage_key, thumbnail_key,
  original_name,
  stored_size, thumbnail_size,
  width, height, thumbnail_width, thumbnail_height,
  content_hash, pixel_hash,
  created_at, updated_at
) VALUES (
  ?, ?, ?,
  ?,
  ?, ?,
  ?, ?, ?, ?,
  ?, ?,
  ?, ?
)
RETURNING *;

-- name: GetActiveImageByPublicID :one
SELECT * FROM images
WHERE public_id = ? AND deleted_at IS NULL
LIMIT 1;

-- name: GetDeletedImageByPublicID :one
SELECT * FROM images
WHERE public_id = ? AND deleted_at IS NOT NULL
LIMIT 1;

-- name: GetActiveImageByPixelHash :one
SELECT * FROM images
WHERE pixel_hash = ? AND deleted_at IS NULL
LIMIT 1;

-- name: ListActiveImages :many
SELECT * FROM images
WHERE deleted_at IS NULL
ORDER BY created_at DESC, id DESC
LIMIT ?;

-- name: ListActiveImagesAfter :many
SELECT * FROM images
WHERE deleted_at IS NULL
  AND (created_at < ? OR (created_at = ? AND id < ?))
ORDER BY created_at DESC, id DESC
LIMIT ?;

-- name: ListDeletedImages :many
SELECT * FROM images
WHERE deleted_at IS NOT NULL
ORDER BY deleted_at DESC, id DESC
LIMIT ?;

-- name: ListDeletedImagesAfter :many
SELECT * FROM images
WHERE deleted_at IS NOT NULL
  AND (deleted_at < ? OR (deleted_at = ? AND id < ?))
ORDER BY deleted_at DESC, id DESC
LIMIT ?;

-- name: SoftDeleteImage :execrows
UPDATE images
SET deleted_at = ?, updated_at = ?
WHERE public_id = ? AND deleted_at IS NULL;

-- name: RestoreImage :execrows
UPDATE images
SET deleted_at = NULL, updated_at = ?
WHERE public_id = ? AND deleted_at IS NOT NULL;

-- name: DeleteImagePermanently :one
DELETE FROM images
WHERE public_id = ? AND deleted_at IS NOT NULL
RETURNING *;
