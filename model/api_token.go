package model

type APIToken struct {
	ID          int64  `json:"id"`
	Name        string `json:"name"`
	TokenPrefix string `json:"token_prefix"` // 仅用于展示和识别，不是可用令牌
	CreatedAt   int64  `json:"created_at"`
	LastUsedAt  *int64 `json:"last_used_at,omitempty"`
	ExpiresAt   *int64 `json:"expires_at,omitempty"`
	RevokedAt   *int64 `json:"revoked_at,omitempty"`
}
