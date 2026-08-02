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

// New 初始化正式目录，并清理上次进程异常退出遗留的临时文件。
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
	// publicID 理论上不会碰撞，这里的检查用于避免意外覆盖已有文件。
	if err := ensureNotExists(imagePath); err != nil {
		return err
	}
	if err := ensureNotExists(thumbnailPath); err != nil {
		return err
	}

	// 两份数据先完整写入临时目录，任意一步失败都会由 defer 清理。
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
	// 临时目录和正式目录位于同一个 data 根目录，rename 在同一文件系统内是原子的。
	if err := os.Rename(temporaryImage, imagePath); err != nil {
		return fmt.Errorf("保存 WebP: %w", err)
	}
	if err := os.Rename(temporaryThumbnail, thumbnailPath); err != nil {
		// 第二次移动失败时撤销第一份文件，避免只留下原图而没有缩略图。
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

	// 文件不存在也视为删除成功，使失败回滚和重复清理具备幂等性。
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
	// Sync 成功后再转正，避免系统异常时留下内容尚未刷盘的正式文件。
	if err := file.Sync(); err != nil {
		return "", err
	}
	if err := file.Close(); err != nil {
		return "", err
	}
	return path, nil
}

func (s *Store) resolve(key string) (string, error) {
	// 只接受相对本地路径，拒绝绝对路径和 ../，防止访问 data 目录之外的文件。
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
