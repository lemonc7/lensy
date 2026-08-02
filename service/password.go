package service

import (
	"crypto/subtle"
	"encoding/base64"
	"errors"
	"fmt"
	"io"
	"strings"

	"golang.org/x/crypto/argon2"
)

const (
	passwordMemory      = 32 * 1024
	passwordIterations  = 2
	passwordParallelism = 1
	passwordSaltLength  = 16
	passwordKeyLength   = 32
)

// hashPassword 使用 Argon2id 生成包含参数和盐值的 PHC 格式密码哈希。
func hashPassword(password string, random io.Reader) (string, error) {
	if err := ValidatePassword(password); err != nil {
		return "", err
	}

	salt := make([]byte, passwordSaltLength)
	if _, err := io.ReadFull(random, salt); err != nil {
		return "", fmt.Errorf("生成密码盐值: %w", err)
	}
	key := argon2.IDKey(
		[]byte(password),
		salt,
		passwordIterations,
		passwordMemory,
		passwordParallelism,
		passwordKeyLength,
	)

	return fmt.Sprintf(
		"$argon2id$v=%d$m=%d,t=%d,p=%d$%s$%s",
		argon2.Version,
		passwordMemory,
		passwordIterations,
		passwordParallelism,
		base64.RawStdEncoding.EncodeToString(salt),
		base64.RawStdEncoding.EncodeToString(key),
	), nil
}

// ValidatePassword 统一定义创建用户时使用的密码长度规则。
// API 可以用它提前返回参数错误，Service 在生成哈希前仍会再次校验。
func ValidatePassword(password string) error {
	if len(password) < 10 || len(password) > 1024 {
		return fmt.Errorf("%w: 密码长度必须为 10 到 1024 字节", ErrInvalidInput)
	}
	return nil
}

// verifyPassword 从数据库哈希中读取参数，再以常量时间比较计算结果。
func verifyPassword(password, encoded string) (bool, error) {
	parts := strings.Split(encoded, "$")
	if len(parts) != 6 || parts[1] != "argon2id" {
		return false, errors.New("无效的 Argon2id 密码哈希")
	}

	var version int
	if _, err := fmt.Sscanf(parts[2], "v=%d", &version); err != nil || version != argon2.Version {
		return false, errors.New("不支持的 Argon2id 版本")
	}

	var memory, iterations uint32
	var parallelism uint8
	if _, err := fmt.Sscanf(parts[3], "m=%d,t=%d,p=%d", &memory, &iterations, &parallelism); err != nil {
		return false, errors.New("无效的 Argon2id 参数")
	}
	// 参数来自数据库，但仍设置上限，防止损坏数据导致异常内存或 CPU 消耗。
	if memory == 0 || memory > 256*1024 || iterations == 0 || iterations > 10 || parallelism == 0 || parallelism > 16 {
		return false, errors.New("Argon2id 参数超出允许范围")
	}

	salt, err := base64.RawStdEncoding.DecodeString(parts[4])
	if err != nil || len(salt) < 8 || len(salt) > 64 {
		return false, errors.New("无效的 Argon2id 盐值")
	}
	expected, err := base64.RawStdEncoding.DecodeString(parts[5])
	if err != nil || len(expected) < 16 || len(expected) > 64 {
		return false, errors.New("无效的 Argon2id 哈希值")
	}

	actual := argon2.IDKey([]byte(password), salt, iterations, memory, parallelism, uint32(len(expected)))
	return subtle.ConstantTimeCompare(actual, expected) == 1, nil
}
