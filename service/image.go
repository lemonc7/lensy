package service

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"strings"
	"time"
	"unicode/utf8"

	imageprocessor "github.com/lemonc7/lensy/image"
	"github.com/lemonc7/lensy/model"
	"github.com/lemonc7/lensy/repo"
	"github.com/lemonc7/lensy/storage"
)

type Image struct {
	queries   *repo.Queries
	processor *imageprocessor.Processor
	storage   *storage.Store
	now       func() time.Time
	random    io.Reader
}

func NewImage(queries *repo.Queries, processor *imageprocessor.Processor, store *storage.Store) *Image {
	return &Image{
		queries: queries, processor: processor, storage: store,
		now: time.Now, random: rand.Reader,
	}
}

type UploadImageInput struct {
	OriginalName string
	Source       io.Reader
}

type UploadImageResult struct {
	Image         model.Image
	AlreadyExists bool
}

type ImagePage struct {
	Images     []model.Image      `json:"images"`
	NextCursor *model.ImageCursor `json:"next_cursor,omitempty"`
}

func (s *Image) Upload(ctx context.Context, input UploadImageInput) (UploadImageResult, error) {
	if err := validateOriginalName(input.OriginalName); err != nil {
		return UploadImageResult{}, err
	}
	if input.Source == nil {
		return UploadImageResult{}, fmt.Errorf("%w: image source is required", ErrInvalidInput)
	}
	if err := ctx.Err(); err != nil {
		return UploadImageResult{}, err
	}

	processed, err := s.processor.Process(input.Source)
	if err != nil {
		return UploadImageResult{}, fmt.Errorf("process image: %w", err)
	}
	if err := ctx.Err(); err != nil {
		return UploadImageResult{}, err
	}

	if existing, err := s.findActiveByPixelHash(ctx, processed.PixelHash); err == nil {
		return UploadImageResult{Image: existing, AlreadyExists: true}, nil
	} else if !errors.Is(err, ErrNotFound) {
		return UploadImageResult{}, fmt.Errorf("find duplicate image: %w", err)
	}

	publicID, err := randomHex(s.random, 16)
	if err != nil {
		return UploadImageResult{}, fmt.Errorf("generate image id: %w", err)
	}
	storageKey := "images/" + publicID[:2] + "/" + publicID + ".webp"
	thumbnailKey := "thumbnails/" + publicID[:2] + "/" + publicID + ".webp"
	if err := s.storage.SaveImage(
		ctx,
		storageKey,
		processed.WebP,
		thumbnailKey,
		processed.ThumbnailWebP,
	); err != nil {
		return UploadImageResult{}, fmt.Errorf("store image: %w", err)
	}

	now := s.now().Unix()
	row, err := s.queries.CreateImage(ctx, repo.CreateImageParams{
		PublicID: publicID, StorageKey: storageKey, ThumbnailKey: thumbnailKey,
		OriginalName: input.OriginalName,
		StoredSize:   int64(len(processed.WebP)), ThumbnailSize: int64(len(processed.ThumbnailWebP)),
		Width: processed.Width, Height: processed.Height,
		ThumbnailWidth: processed.ThumbnailWidth, ThumbnailHeight: processed.ThumbnailHeight,
		ContentHash: processed.ContentHash, PixelHash: processed.PixelHash,
		CreatedAt: now, UpdatedAt: now,
	})
	if err == nil {
		return UploadImageResult{Image: imageFromRow(row)}, nil
	}

	if cleanupErr := s.storage.RemoveImage(storageKey, thumbnailKey); cleanupErr != nil {
		return UploadImageResult{}, errors.Join(
			fmt.Errorf("create image: %w", err),
			fmt.Errorf("rollback stored image: %w", cleanupErr),
		)
	}

	// 部分唯一索引是并发上传相同图片时的最终保护。
	if existing, lookupErr := s.findActiveByPixelHash(ctx, processed.PixelHash); lookupErr == nil {
		return UploadImageResult{Image: existing, AlreadyExists: true}, nil
	}
	return UploadImageResult{}, fmt.Errorf("create image: %w", err)
}

func (s *Image) Get(ctx context.Context, publicID string) (model.Image, error) {
	row, err := s.queries.GetActiveImageByPublicID(ctx, publicID)
	return imageFromRow(row), mapNotFound(err)
}

func (s *Image) List(ctx context.Context, cursor *model.ImageCursor, limit int) (ImagePage, error) {
	return s.list(ctx, cursor, limit, false)
}

func (s *Image) ListTrash(ctx context.Context, cursor *model.ImageCursor, limit int) (ImagePage, error) {
	return s.list(ctx, cursor, limit, true)
}

func (s *Image) list(ctx context.Context, cursor *model.ImageCursor, limit int, deleted bool) (ImagePage, error) {
	limit = clampLimit(limit)
	var images []model.Image
	var err error
	if deleted {
		images, err = s.listDeleted(ctx, cursor, int64(limit+1))
	} else {
		images, err = s.listActive(ctx, cursor, int64(limit+1))
	}
	if err != nil {
		return ImagePage{}, fmt.Errorf("list images: %w", err)
	}
	page := ImagePage{Images: images}
	if len(images) > limit {
		last := images[limit-1]
		timestamp := last.CreatedAt
		if deleted && last.DeletedAt != nil {
			timestamp = *last.DeletedAt
		}
		page.Images = images[:limit]
		page.NextCursor = &model.ImageCursor{Timestamp: timestamp, ID: last.ID}
	}
	return page, nil
}

func (s *Image) SoftDelete(ctx context.Context, publicID string) error {
	now := s.now().Unix()
	rows, err := s.queries.SoftDeleteImage(ctx, repo.SoftDeleteImageParams{
		DeletedAt: &now, UpdatedAt: now, PublicID: publicID,
	})
	if err != nil {
		return fmt.Errorf("soft delete image: %w", err)
	}
	if rows == 0 {
		return ErrNotFound
	}
	return nil
}

func (s *Image) Restore(ctx context.Context, publicID string) (model.Image, error) {
	deletedRow, err := s.queries.GetDeletedImageByPublicID(ctx, publicID)
	if err != nil {
		return model.Image{}, mapNotFound(err)
	}
	deleted := imageFromRow(deletedRow)
	if existing, findErr := s.findActiveByPixelHash(ctx, deleted.PixelHash); findErr == nil {
		return model.Image{}, &RestoreConflictError{ExistingPublicID: existing.PublicID}
	} else if !errors.Is(findErr, ErrNotFound) {
		return model.Image{}, fmt.Errorf("check restore conflict: %w", findErr)
	}

	rows, err := s.queries.RestoreImage(ctx, repo.RestoreImageParams{UpdatedAt: s.now().Unix(), PublicID: publicID})
	if err != nil {
		if existing, findErr := s.findActiveByPixelHash(ctx, deleted.PixelHash); findErr == nil {
			return model.Image{}, &RestoreConflictError{ExistingPublicID: existing.PublicID}
		}
		return model.Image{}, fmt.Errorf("restore image: %w", err)
	}
	if rows == 0 {
		return model.Image{}, ErrNotFound
	}
	return s.Get(ctx, publicID)
}

func (s *Image) DeletePermanently(ctx context.Context, publicID string) (model.Image, error) {
	row, err := s.queries.DeleteImagePermanently(ctx, publicID)
	if err != nil {
		return model.Image{}, mapNotFound(err)
	}
	image := imageFromRow(row)
	if err := s.storage.RemoveImage(image.StorageKey, image.ThumbnailKey); err != nil {
		return image, fmt.Errorf("remove stored image: %w", err)
	}
	return image, nil
}

func validateOriginalName(name string) error {
	if strings.TrimSpace(name) == "" || utf8.RuneCountInString(name) > 255 {
		return fmt.Errorf("%w: original name must contain 1-255 characters", ErrInvalidInput)
	}
	return nil
}

func randomHex(reader io.Reader, byteCount int) (string, error) {
	buffer := make([]byte, byteCount)
	if _, err := io.ReadFull(reader, buffer); err != nil {
		return "", err
	}
	return hex.EncodeToString(buffer), nil
}

func clampLimit(limit int) int {
	if limit <= 0 {
		return 20
	}
	if limit > 100 {
		return 100
	}
	return limit
}

func (s *Image) findActiveByPixelHash(ctx context.Context, hash string) (model.Image, error) {
	row, err := s.queries.GetActiveImageByPixelHash(ctx, hash)
	return imageFromRow(row), mapNotFound(err)
}

func (s *Image) listActive(ctx context.Context, cursor *model.ImageCursor, limit int64) ([]model.Image, error) {
	var rows []repo.Image
	var err error
	if cursor == nil {
		rows, err = s.queries.ListActiveImages(ctx, limit)
	} else {
		rows, err = s.queries.ListActiveImagesAfter(ctx, repo.ListActiveImagesAfterParams{
			CreatedAt: cursor.Timestamp, CreatedAt_2: cursor.Timestamp, ID: cursor.ID, Limit: limit,
		})
	}
	return imagesFromRows(rows), err
}

func (s *Image) listDeleted(ctx context.Context, cursor *model.ImageCursor, limit int64) ([]model.Image, error) {
	var rows []repo.Image
	var err error
	if cursor == nil {
		rows, err = s.queries.ListDeletedImages(ctx, limit)
	} else {
		rows, err = s.queries.ListDeletedImagesAfter(ctx, repo.ListDeletedImagesAfterParams{
			DeletedAt: &cursor.Timestamp, DeletedAt_2: &cursor.Timestamp, ID: cursor.ID, Limit: limit,
		})
	}
	return imagesFromRows(rows), err
}

func imageFromRow(row repo.Image) model.Image {
	return model.Image{
		ID: row.ID, PublicID: row.PublicID, StorageKey: row.StorageKey, ThumbnailKey: row.ThumbnailKey,
		OriginalName: row.OriginalName, StoredSize: row.StoredSize, ThumbnailSize: row.ThumbnailSize,
		Width: row.Width, Height: row.Height, ThumbnailWidth: row.ThumbnailWidth, ThumbnailHeight: row.ThumbnailHeight,
		ContentHash: row.ContentHash, PixelHash: row.PixelHash,
		CreatedAt: row.CreatedAt, UpdatedAt: row.UpdatedAt, DeletedAt: row.DeletedAt,
	}
}

func imagesFromRows(rows []repo.Image) []model.Image {
	images := make([]model.Image, len(rows))
	for i := range rows {
		images[i] = imageFromRow(rows[i])
	}
	return images
}
