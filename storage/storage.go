package storage

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"
)

const storageRoot = "data"

var ErrInvalidKey = errors.New("无效的存储键")

type Store struct {
	root    string
	tempDir string
}

func New() (*Store, error) {
	root, err := filepath.Abs(storageRoot)
	if err != nil {
		return nil, fmt.Errorf("解析存储目录: %w", err)
	}

	store := &Store{root: root, tempDir: filepath.Join(root, "tmp")}
	if err := os.RemoveAll(store.tempDir); err != nil {
		return nil, fmt.Errorf("清理临时目录: %w", err)
	}
	for _, dir := range []string{
		filepath.Join(root, "images"),
		filepath.Join(root, "thumbnails"),
		store.tempDir,
	} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return nil, fmt.Errorf("创建存储目录: %w", err)
		}
	}
	return store, nil
}

func (s *Store) SaveImage(
	ctx context.Context,
	imageKey string,
	imageData []byte,
	thumbnailKey string,
	thumbnailData []byte,
) error {
	if err := ctx.Err(); err != nil {
		return err
	}

	imagePath, err := s.resolve(imageKey)
	if err != nil {
		return err
	}
	thumbnailPath, err := s.resolve(thumbnailKey)
	if err != nil {
		return err
	}
	if err := ensureNotExists(imagePath); err != nil {
		return err
	}
	if err := ensureNotExists(thumbnailPath); err != nil {
		return err
	}

	temporaryImage, err := s.writeTemporary(imageData)
	if err != nil {
		return fmt.Errorf("写入临时 WebP: %w", err)
	}
	defer os.Remove(temporaryImage)

	temporaryThumbnail, err := s.writeTemporary(thumbnailData)
	if err != nil {
		return fmt.Errorf("写入临时缩略图: %w", err)
	}
	defer os.Remove(temporaryThumbnail)

	if err := ctx.Err(); err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(imagePath), 0o755); err != nil {
		return fmt.Errorf("创建图片目录: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(thumbnailPath), 0o755); err != nil {
		return fmt.Errorf("创建缩略图目录: %w", err)
	}
	if err := os.Rename(temporaryImage, imagePath); err != nil {
		return fmt.Errorf("保存 WebP: %w", err)
	}
	if err := os.Rename(temporaryThumbnail, thumbnailPath); err != nil {
		if removeErr := os.Remove(imagePath); removeErr != nil && !errors.Is(removeErr, os.ErrNotExist) {
			return errors.Join(
				fmt.Errorf("保存缩略图: %w", err),
				fmt.Errorf("回滚 WebP: %w", removeErr),
			)
		}
		return fmt.Errorf("保存缩略图: %w", err)
	}
	return nil
}

func (s *Store) RemoveImage(imageKey, thumbnailKey string) error {
	imagePath, err := s.resolve(imageKey)
	if err != nil {
		return err
	}
	thumbnailPath, err := s.resolve(thumbnailKey)
	if err != nil {
		return err
	}

	var removeErrors []error
	for _, path := range []string{imagePath, thumbnailPath} {
		if err := os.Remove(path); err != nil && !errors.Is(err, os.ErrNotExist) {
			removeErrors = append(removeErrors, err)
		}
	}
	return errors.Join(removeErrors...)
}

func (s *Store) Open(key string) (*os.File, error) {
	path, err := s.resolve(key)
	if err != nil {
		return nil, err
	}
	return os.Open(path)
}

func (s *Store) writeTemporary(data []byte) (path string, err error) {
	file, err := os.CreateTemp(s.tempDir, "lensy-*.tmp")
	if err != nil {
		return "", err
	}
	path = file.Name()
	defer func() {
		if err != nil {
			file.Close()
			os.Remove(path)
		}
	}()

	if _, err := file.Write(data); err != nil {
		return "", err
	}
	if err := file.Sync(); err != nil {
		return "", err
	}
	if err := file.Close(); err != nil {
		return "", err
	}
	return path, nil
}

func (s *Store) resolve(key string) (string, error) {
	if key == "." || !filepath.IsLocal(key) {
		return "", ErrInvalidKey
	}
	return filepath.Join(s.root, key), nil
}

func ensureNotExists(path string) error {
	_, err := os.Lstat(path)
	switch {
	case err == nil:
		return os.ErrExist
	case errors.Is(err, os.ErrNotExist):
		return nil
	default:
		return err
	}
}
