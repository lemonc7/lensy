package service

import (
	"context"
	"crypto/rand"
	"database/sql"
	"errors"
	"fmt"
	"io"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/lemonc7/lensy/model"
	"github.com/lemonc7/lensy/repo"
	"golang.org/x/crypto/argon2"
)

const (
	usernameMinLength = 3
	usernameMaxLength = 100
)

type Auth struct {
	queries *repo.Queries
	now     func() time.Time
	random  io.Reader
}

func NewAuth(queries *repo.Queries) *Auth {
	return &Auth{queries: queries, now: time.Now, random: rand.Reader}
}

// CreateUser 创建用户，数据库只保存包含参数和随机盐值的 Argon2id 密码哈希。
// HTTP 层不公开注册接口，初始管理员应由单独的初始化命令创建。
func (s *Auth) CreateUser(ctx context.Context, username, password string) (model.User, error) {
	username, err := normalizeUsername(username)
	if err != nil {
		return model.User{}, err
	}
	if _, err := s.queries.GetUserByUsername(ctx, username); err == nil {
		return model.User{}, ErrUserExists
	} else if !errors.Is(err, sql.ErrNoRows) {
		return model.User{}, fmt.Errorf("检查用户名: %w", err)
	}

	passwordHash, err := hashPassword(password, s.random)
	if err != nil {
		return model.User{}, err
	}
	now := s.now().Unix()
	row, err := s.queries.CreateUser(ctx, repo.CreateUserParams{
		Username: username, PasswordHash: passwordHash,
		CreatedAt: now, UpdatedAt: now,
	})
	if err != nil {
		// 前面的查询提供友好提示，数据库唯一约束负责阻止并发创建同名用户。
		return model.User{}, fmt.Errorf("创建用户: %w", err)
	}
	return userFromRow(row), nil
}

// Login 校验用户名和密码。Session 的创建与 Cookie 写入由 HTTP 层负责。
func (s *Auth) Login(ctx context.Context, username, password string) (model.User, error) {
	username, err := normalizeUsername(username)
	if err != nil || len(password) > 1024 {
		return model.User{}, ErrInvalidCredentials
	}

	row, err := s.queries.GetUserByUsername(ctx, username)
	if errors.Is(err, sql.ErrNoRows) {
		// 即使用户名不存在也执行一次 Argon2id，缩小用户名枚举的时间差。
		consumePasswordTime(password)
		return model.User{}, ErrInvalidCredentials
	}
	if err != nil {
		return model.User{}, fmt.Errorf("查询用户: %w", err)
	}

	matched, err := verifyPassword(password, row.PasswordHash)
	if err != nil {
		return model.User{}, fmt.Errorf("校验密码: %w", err)
	}
	if !matched {
		return model.User{}, ErrInvalidCredentials
	}
	if row.DisabledAt != nil {
		return model.User{}, ErrUserDisabled
	}

	now := s.now().Unix()
	rows, err := s.queries.UpdateUserLastLogin(ctx, repo.UpdateUserLastLoginParams{
		LastLoginAt: &now, UpdatedAt: now, ID: row.ID,
	})
	if err != nil {
		return model.User{}, fmt.Errorf("更新最后登录时间: %w", err)
	}
	if rows == 0 {
		// 用户可能在密码校验期间被另一个请求禁用。
		return model.User{}, ErrUserDisabled
	}

	user := userFromRow(row)
	user.LastLoginAt = &now
	user.UpdatedAt = now
	return user, nil
}

func (s *Auth) GetUser(ctx context.Context, id int64) (model.User, error) {
	row, err := s.queries.GetUserByID(ctx, id)
	if err != nil {
		return model.User{}, mapNotFound(err)
	}
	if row.DisabledAt != nil {
		return model.User{}, ErrUserDisabled
	}
	return userFromRow(row), nil
}

func (s *Auth) SetupRequired(ctx context.Context) (bool, error) {
	count, err := s.queries.CountUsers(ctx)
	if err != nil {
		return false, fmt.Errorf("统计用户数量: %w", err)
	}

	return count == 0, nil
}

func (s *Auth) CreateFirstAdmin(
	ctx context.Context,
	username string,
	password string,
) (model.User, error) {
	required, err := s.SetupRequired(ctx)
	if err != nil {
		return model.User{}, err
	}
	if !required {
		return model.User{}, ErrAdminInitialized
	}
	username, err = normalizeUsername(username)
	if err != nil {
		return model.User{}, err
	}

	passwordHash, err := hashPassword(password, s.random)
	if err != nil {
		return model.User{}, err
	}

	now := s.now().Unix()
	row, err := s.queries.CreateFirstAdmin(
		ctx,
		repo.CreateFirstAdminParams{
			Username:     username,
			PasswordHash: passwordHash,
			CreatedAt:    now,
			UpdatedAt:    now,
		},
	)
	if errors.Is(err, sql.ErrNoRows) {
		return model.User{}, ErrAdminInitialized
	}
	if err != nil {
		return model.User{}, fmt.Errorf("初始化管理员: %w", err)
	}
	return userFromRow(row), nil
}

// ValidateUsername 暴露统一的用户名规则，供 API 在调用业务前尽早反馈参数错误。
func ValidateUsername(username string) error {
	_, err := normalizeUsername(username)
	return err
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

func consumePasswordTime(password string) {
	// 固定盐值只用于模拟密码校验开销，结果不会用于存储或认证。
	argon2.IDKey([]byte(password), make([]byte, passwordSaltLength), passwordIterations, passwordMemory, passwordParallelism, passwordKeyLength)
}

func userFromRow(row repo.User) model.User {
	return model.User{
		ID: row.ID, Username: row.Username,
		CreatedAt: row.CreatedAt, UpdatedAt: row.UpdatedAt,
		LastLoginAt: row.LastLoginAt, DisabledAt: row.DisabledAt,
	}
}
