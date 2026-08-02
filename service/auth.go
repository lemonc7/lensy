package service

import (
	"context"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"fmt"
	"strings"
	"unicode/utf8"

	"github.com/lemonc7/lensy/model"
)

const (
	usernameMinLength = 3
	usernameMaxLength = 100
)

// Auth 使用配置文件中的单一管理员账号，不依赖数据库用户表。
type Auth struct {
	username       string
	passwordHash   string
	sessionVersion string
}

func NewAuth(username, passwordHash string) (*Auth, error) {
	username, err := normalizeUsername(username)
	if err != nil {
		return nil, err
	}
	// 启动时完整解析一次 PHC 字符串，尽早发现配置中的损坏哈希。
	if _, err := verifyPassword("", passwordHash); err != nil {
		return nil, fmt.Errorf("管理员密码哈希无效: %w", err)
	}
	version := sha256.Sum256([]byte(passwordHash))
	return &Auth{
		username:       username,
		passwordHash:   passwordHash,
		sessionVersion: hex.EncodeToString(version[:16]),
	}, nil
}

// Login 始终执行一次 Argon2id 校验，避免通过响应耗时判断用户名是否正确。
func (s *Auth) Login(ctx context.Context, username, password string) (model.User, error) {
	if err := ctx.Err(); err != nil {
		return model.User{}, err
	}
	if len(password) > 1024 {
		return model.User{}, ErrInvalidCredentials
	}

	matched, err := verifyPassword(password, s.passwordHash)
	if err != nil {
		return model.User{}, fmt.Errorf("校验管理员密码: %w", err)
	}
	username = strings.TrimSpace(username)
	usernameMatched := subtle.ConstantTimeCompare(
		[]byte(strings.ToLower(username)),
		[]byte(strings.ToLower(s.username)),
	) == 1
	if !usernameMatched || !matched {
		return model.User{}, ErrInvalidCredentials
	}
	return s.User(), nil
}

func (s *Auth) User() model.User {
	return model.User{Username: s.username}
}

func (s *Auth) SessionVersion() string {
	return s.sessionVersion
}

func normalizeUsername(username string) (string, error) {
	username = strings.TrimSpace(username)
	length := utf8.RuneCountInString(username)
	if length < usernameMinLength || length > usernameMaxLength {
		return "", fmt.Errorf(
			"%w: 用户名长度必须为 %d 到 %d 个字符",
			ErrInvalidInput,
			usernameMinLength,
			usernameMaxLength,
		)
	}
	return username, nil
}
