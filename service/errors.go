package service

import (
	"database/sql"
	"errors"
)

var (
	ErrNotFound        = errors.New("not found")
	ErrRestoreConflict = errors.New("an active image with the same pixels already exists")
	ErrInvalidInput    = errors.New("invalid input")
	ErrInvalidToken    = errors.New("invalid API token")
	ErrExpiredToken    = errors.New("API token has expired")
	ErrRevokedToken    = errors.New("API token has been revoked")
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
