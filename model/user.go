package model

// User 表示配置文件中唯一的管理员，不包含密码哈希。
type User struct {
	Username string `json:"username"`
}
