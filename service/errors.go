package service

import (
	"database/sql"
	"errors"
)

var (
	ErrNotFound           = errors.New("未找到记录")
	ErrRestoreConflict    = errors.New("已存在像素内容相同的有效图片")
	ErrInvalidInput       = errors.New("输入参数无效")
	ErrInvalidToken       = errors.New("API Token 无效")
	ErrExpiredToken       = errors.New("API Token 已过期")
	ErrRevokedToken       = errors.New("API Token 已撤销")
	ErrInvalidCredentials = errors.New("用户名或密码错误")
)

type RestoreConflictError struct {
	ExistingPublicID string
}

func (e *RestoreConflictError) Error() string {
	return ErrRestoreConflict.Error() + ": " + e.ExistingPublicID
}

func (e *RestoreConflictError) Unwrap() error { return ErrRestoreConflict }

func mapNotFound(err error) error {
	if errors.Is(err, sql.ErrNoRows) {
		return ErrNotFound
	}
	return err
}
