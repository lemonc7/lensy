-- name: CreateUser :one
INSERT INTO users (
  username, password_hash, created_at, updated_at
) VALUES (?, ?, ?, ?)
RETURNING *;

-- name: GetUserByID :one
SELECT * FROM users
WHERE id = ?
LIMIT 1;

-- name: GetUserByUsername :one
SELECT * FROM users
WHERE username = ? COLLATE NOCASE
LIMIT 1;

-- name: UpdateUserLastLogin :execrows
UPDATE users
SET last_login_at = ?, updated_at = ?
WHERE id = ? AND disabled_at IS NULL;

-- name: CountUsers :one
SELECT COUNT(*) FROM users;

-- name: CreateFirstAdmin :one
INSERT INTO users (
  username,
  password_hash,
  created_at,
  updated_at
)
SELECT
  sqlc.arg(username),
  sqlc.arg(password_hash),
  sqlc.arg(created_at),
  sqlc.arg(updated_at)
WHERE NOT EXISTS (
  SELECT 1 FROM users
)
RETURNING *;
  