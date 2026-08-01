-- name: CreateAPIToken :one
INSERT INTO api_tokens (
  name, token_prefix, token_hash, created_at, expires_at
) VALUES (?, ?, ?, ?, ?)
RETURNING *;

-- name: GetAPITokenByHash :one
SELECT * FROM api_tokens
WHERE token_hash = ?
LIMIT 1;

-- name: ListAPITokens :many
SELECT * FROM api_tokens
ORDER BY created_at DESC, id DESC;

-- name: TouchAPIToken :execrows
UPDATE api_tokens
SET last_used_at = ?
WHERE id = ?
  AND revoked_at IS NULL
  AND (expires_at IS NULL OR expires_at > ?);

-- name: RevokeAPIToken :execrows
UPDATE api_tokens
SET revoked_at = ?
WHERE id = ? AND revoked_at IS NULL;
