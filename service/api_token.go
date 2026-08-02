package service

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/lemonc7/lensy/model"
	"github.com/lemonc7/lensy/repo"
)

type APIToken struct {
	queries *repo.Queries
	now     func() time.Time
	random  io.Reader
}

const (
	apiTokenPrefix     = "lensy_"
	apiTokenRandomSize = 32
)

func NewAPIToken(queries *repo.Queries) *APIToken {
	return &APIToken{queries: queries, now: time.Now, random: rand.Reader}
}

type IssuedAPIToken struct {
	Token  string         `json:"token"` // 明文只在创建成功时返回这一次
	Record model.APIToken `json:"record"`
}

// Issue 创建一个令牌。TTL 为零表示永不过期。
// 原始令牌只返回一次，数据库仅保存它的 SHA-256 哈希。
func (s *APIToken) Issue(ctx context.Context, name string, ttl time.Duration) (IssuedAPIToken, error) {
	name = strings.TrimSpace(name)
	if name == "" || utf8.RuneCountInString(name) > 100 {
		return IssuedAPIToken{}, fmt.Errorf("%w: Token 名称长度必须为 1 到 100 个字符", ErrInvalidInput)
	}
	if ttl < 0 {
		return IssuedAPIToken{}, fmt.Errorf("%w: Token 有效期不能为负数", ErrInvalidInput)
	}

	// 32 字节密码学随机数提供足够熵，URL-safe Base64 可直接放进 Authorization 头。
	rawBytes := make([]byte, apiTokenRandomSize)
	if _, err := io.ReadFull(s.random, rawBytes); err != nil {
		return IssuedAPIToken{}, fmt.Errorf("生成 API Token: %w", err)
	}
	raw := apiTokenPrefix + base64.RawURLEncoding.EncodeToString(rawBytes)
	// 数据库只保存哈希；即使数据库泄露，也不能直接得到可用令牌。
	hash := tokenHash(raw)
	now := s.now()
	var expiresAt *int64
	if ttl > 0 {
		value := now.Add(ttl).Unix()
		expiresAt = &value
	}

	row, err := s.queries.CreateAPIToken(ctx, repo.CreateAPITokenParams{
		// 前缀仅用于后台展示和区分令牌，不具备认证能力。
		Name: name, TokenPrefix: raw[:12], TokenHash: hash,
		CreatedAt: now.Unix(), ExpiresAt: expiresAt,
	})
	if err != nil {
		return IssuedAPIToken{}, fmt.Errorf("创建 API Token: %w", err)
	}
	return IssuedAPIToken{Token: raw, Record: tokenFromRow(row)}, nil
}

func (s *APIToken) Authenticate(ctx context.Context, raw string) (model.APIToken, error) {
	expectedLength := len(apiTokenPrefix) + base64.RawURLEncoding.EncodedLen(apiTokenRandomSize)
	if !strings.HasPrefix(raw, apiTokenPrefix) || len(raw) != expectedLength {
		return model.APIToken{}, ErrInvalidToken
	}
	row, err := s.queries.GetAPITokenByHash(ctx, tokenHash(raw))
	if mapped := mapNotFound(err); errors.Is(mapped, ErrNotFound) {
		return model.APIToken{}, ErrInvalidToken
	}
	if err != nil {
		return model.APIToken{}, fmt.Errorf("查询 API Token: %w", err)
	}
	stored := tokenFromRow(row)
	if stored.RevokedAt != nil {
		return model.APIToken{}, ErrRevokedToken
	}
	now := s.now().Unix()
	if stored.ExpiresAt != nil && *stored.ExpiresAt <= now {
		return model.APIToken{}, ErrExpiredToken
	}
	// 更新语句会再次检查撤销和过期状态，防止查询后、更新前状态发生变化。
	rows, err := s.queries.TouchAPIToken(ctx, repo.TouchAPITokenParams{
		LastUsedAt: &now, ID: stored.ID, ExpiresAt: &now,
	})
	if err != nil {
		return model.APIToken{}, fmt.Errorf("更新 API Token 使用时间: %w", err)
	}
	if rows == 0 {
		// 令牌可能在查询和更新时间之间被撤销。
		return model.APIToken{}, ErrInvalidToken
	}
	stored.LastUsedAt = &now
	return stored, nil
}

func (s *APIToken) List(ctx context.Context) ([]model.APIToken, error) {
	rows, err := s.queries.ListAPITokens(ctx)
	if err != nil {
		return nil, fmt.Errorf("查询 API Token 列表: %w", err)
	}
	tokens := make([]model.APIToken, len(rows))
	for i := range rows {
		tokens[i] = tokenFromRow(rows[i])
	}
	return tokens, nil
}

func (s *APIToken) Revoke(ctx context.Context, id int64) error {
	if id <= 0 {
		return fmt.Errorf("%w: Token ID 必须为正数", ErrInvalidInput)
	}
	now := s.now().Unix()
	rows, err := s.queries.RevokeAPIToken(ctx, repo.RevokeAPITokenParams{RevokedAt: &now, ID: id})
	if err != nil {
		return fmt.Errorf("撤销 API Token: %w", err)
	}
	if rows == 0 {
		return ErrNotFound
	}
	return nil
}

func tokenHash(raw string) string {
	sum := sha256.Sum256([]byte(raw))
	return hex.EncodeToString(sum[:])
}

func tokenFromRow(row repo.ApiToken) model.APIToken {
	return model.APIToken{
		ID: row.ID, Name: row.Name, TokenPrefix: row.TokenPrefix,
		CreatedAt: row.CreatedAt, LastUsedAt: row.LastUsedAt,
		ExpiresAt: row.ExpiresAt, RevokedAt: row.RevokedAt,
	}
}
