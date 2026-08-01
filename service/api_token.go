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

func NewAPIToken(queries *repo.Queries) *APIToken {
	return &APIToken{queries: queries, now: time.Now, random: rand.Reader}
}

type IssuedAPIToken struct {
	Token  string         `json:"token"`
	Record model.APIToken `json:"record"`
}

// Issue 创建一个令牌。TTL 为零表示永不过期。
// 原始令牌只返回一次，数据库仅保存它的 SHA-256 哈希。
func (s *APIToken) Issue(ctx context.Context, name string, ttl time.Duration) (IssuedAPIToken, error) {
	name = strings.TrimSpace(name)
	if name == "" || utf8.RuneCountInString(name) > 100 {
		return IssuedAPIToken{}, fmt.Errorf("%w: token name must contain 1-100 characters", ErrInvalidInput)
	}
	if ttl < 0 {
		return IssuedAPIToken{}, fmt.Errorf("%w: token TTL cannot be negative", ErrInvalidInput)
	}

	rawBytes := make([]byte, 32)
	if _, err := io.ReadFull(s.random, rawBytes); err != nil {
		return IssuedAPIToken{}, fmt.Errorf("generate API token: %w", err)
	}
	raw := "lensy_" + base64.RawURLEncoding.EncodeToString(rawBytes)
	hash := tokenHash(raw)
	now := s.now()
	var expiresAt *int64
	if ttl > 0 {
		value := now.Add(ttl).Unix()
		expiresAt = &value
	}

	row, err := s.queries.CreateAPIToken(ctx, repo.CreateAPITokenParams{
		Name: name, TokenPrefix: raw[:12], TokenHash: hash,
		CreatedAt: now.Unix(), ExpiresAt: expiresAt,
	})
	if err != nil {
		return IssuedAPIToken{}, fmt.Errorf("create API token: %w", err)
	}
	return IssuedAPIToken{Token: raw, Record: tokenFromRow(row)}, nil
}

func (s *APIToken) Authenticate(ctx context.Context, raw string) (model.APIToken, error) {
	if !strings.HasPrefix(raw, "lensy_") || len(raw) < 20 {
		return model.APIToken{}, ErrInvalidToken
	}
	row, err := s.queries.GetAPITokenByHash(ctx, tokenHash(raw))
	if mapped := mapNotFound(err); errors.Is(mapped, ErrNotFound) {
		return model.APIToken{}, ErrInvalidToken
	}
	if err != nil {
		return model.APIToken{}, fmt.Errorf("find API token: %w", err)
	}
	stored := tokenFromRow(row)
	if stored.RevokedAt != nil {
		return model.APIToken{}, ErrRevokedToken
	}
	now := s.now().Unix()
	if stored.ExpiresAt != nil && *stored.ExpiresAt <= now {
		return model.APIToken{}, ErrExpiredToken
	}
	rows, err := s.queries.TouchAPIToken(ctx, repo.TouchAPITokenParams{
		LastUsedAt: &now, ID: stored.ID, ExpiresAt: &now,
	})
	if err != nil {
		return model.APIToken{}, fmt.Errorf("update API token usage: %w", err)
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
		return nil, fmt.Errorf("list API tokens: %w", err)
	}
	tokens := make([]model.APIToken, len(rows))
	for i := range rows {
		tokens[i] = tokenFromRow(rows[i])
	}
	return tokens, nil
}

func (s *APIToken) Revoke(ctx context.Context, id int64) error {
	if id <= 0 {
		return fmt.Errorf("%w: token id must be positive", ErrInvalidInput)
	}
	now := s.now().Unix()
	rows, err := s.queries.RevokeAPIToken(ctx, repo.RevokeAPITokenParams{RevokedAt: &now, ID: id})
	if err != nil {
		return fmt.Errorf("revoke API token: %w", err)
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
