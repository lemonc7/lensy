package model

// User 是可以安全返回给 HTTP 客户端的用户信息，不包含密码哈希。
type User struct {
	ID          int64  `json:"id"`
	Username    string `json:"username"`
	CreatedAt   int64  `json:"created_at"`
	UpdatedAt   int64  `json:"updated_at"`
	LastLoginAt *int64 `json:"last_login_at,omitempty"`
	DisabledAt  *int64 `json:"disabled_at,omitempty"`
}
