package service

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"io"
	"os"
	"path"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/lemonc7/lensy/image"
	"github.com/lemonc7/lensy/model"
	"github.com/lemonc7/lensy/repo"
	"github.com/lemonc7/lensy/storage"
)

type Image struct {
	queries   *repo.Queries
	processor *image.Processor
	storage   *storage.Store
	now       func() time.Time
	random    io.Reader
}

func NewImage(queries *repo.Queries, processor *image.Processor, store *storage.Store) *Image {
	return &Image{
		queries: queries, processor: processor, storage: store,
		now: time.Now, random: rand.Reader,
	}
}

// UploadImageInput 由 HTTP 层构造，service 接管从读取到持久化的完整流程。
type UploadImageInput struct {
	OriginalName string
	Source       io.Reader
}

type UploadImageResult struct {
	Image         model.Image `json:"image"`
	AlreadyExists bool        `json:"already_exists"` // 为 true 时返回的是数据库中已有的同像素图片
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
		return UploadImageResult{}, fmt.Errorf("%w: 图片来源不能为空", ErrInvalidInput)
	}
	if err := ctx.Err(); err != nil {
		return UploadImageResult{}, err
	}

	// 必须先解码才能得到 PixelHash，因此查重发生在图片处理之后。
	// 编解码库本身不支持中途取消，处理完成后立即再次检查请求状态。
	processed, err := s.processor.Process(input.Source)
	if err != nil {
		return UploadImageResult{}, fmt.Errorf("处理图片: %w", err)
	}
	if err := ctx.Err(); err != nil {
		return UploadImageResult{}, err
	}

	// 第一次查重用于避免正常情况下重复写文件。
	if existing, err := s.findActiveByPixelHash(ctx, processed.PixelHash); err == nil {
		return UploadImageResult{Image: existing, AlreadyExists: true}, nil
	} else if !errors.Is(err, ErrNotFound) {
		return UploadImageResult{}, fmt.Errorf("查询重复图片: %w", err)
	}

	// 固定 12 位 Base62 ID 不含标点，便于复制和展示，同时保留约 71.45 位随机熵。
	publicID, err := randomImageID(s.random)
	if err != nil {
		return UploadImageResult{}, fmt.Errorf("生成图片 ID: %w", err)
	}
	now := s.now()
	// 按月分目录便于人工查看、迁移和备份；常规图片量下无需再拆分到每天。
	datePath := now.Format("2006/01")
	storageKey := path.Join("images", datePath, publicID+".webp")
	thumbnailKey := path.Join("thumbnails", datePath, publicID+".webp")
	// 文件先落盘，数据库后写入；数据库失败时可以根据已知 key 删除文件进行补偿。
	if err := s.storage.SaveImage(
		ctx,
		storageKey,
		processed.WebP,
		thumbnailKey,
		processed.ThumbnailWebP,
	); err != nil {
		return UploadImageResult{}, fmt.Errorf("存储图片: %w", err)
	}

	nowUnix := now.Unix()
	row, err := s.queries.CreateImage(ctx, repo.CreateImageParams{
		PublicID: publicID, StorageKey: storageKey, ThumbnailKey: thumbnailKey,
		OriginalName: input.OriginalName,
		StoredSize:   int64(len(processed.WebP)), ThumbnailSize: int64(len(processed.ThumbnailWebP)),
		Width: processed.Width, Height: processed.Height,
		ThumbnailWidth: processed.ThumbnailWidth, ThumbnailHeight: processed.ThumbnailHeight,
		ContentHash: processed.ContentHash, PixelHash: processed.PixelHash,
		CreatedAt: nowUnix, UpdatedAt: nowUnix,
	})
	if err == nil {
		return UploadImageResult{Image: imageFromRow(row)}, nil
	}

	// 文件系统和 SQLite 无法共享事务，所以这里显式回滚已经保存的两份文件。
	if cleanupErr := s.storage.RemoveImage(storageKey, thumbnailKey); cleanupErr != nil {
		return UploadImageResult{}, errors.Join(
			fmt.Errorf("创建图片记录: %w", err),
			fmt.Errorf("回滚已存储图片: %w", cleanupErr),
		)
	}

	// 部分唯一索引是并发上传相同图片时的最终保护。
	if existing, lookupErr := s.findActiveByPixelHash(ctx, processed.PixelHash); lookupErr == nil {
		return UploadImageResult{Image: existing, AlreadyExists: true}, nil
	}
	return UploadImageResult{}, fmt.Errorf("创建图片记录: %w", err)
}

func (s *Image) Get(ctx context.Context, publicID string) (model.Image, error) {
	row, err := s.queries.GetActiveImageByPublicID(ctx, publicID)
	return imageFromRow(row), mapNotFound(err)
}

// OpenFile 只打开仍处于有效状态的图片文件，软删除后的图片不能通过公开地址读取。
func (s *Image) OpenFile(ctx context.Context, publicID string, thumbnail bool) (model.Image, *os.File, error) {
	storedImage, err := s.Get(ctx, publicID)
	if err != nil {
		return model.Image{}, nil, err
	}

	key := storedImage.StorageKey
	if thumbnail {
		key = storedImage.ThumbnailKey
	}
	file, err := s.storage.Open(key)
	if err != nil {
		return model.Image{}, nil, fmt.Errorf("打开图片文件: %w", err)
	}
	return storedImage, file, nil
}

func (s *Image) List(ctx context.Context, cursor *model.ImageCursor, limit int) (ImagePage, error) {
	return s.list(ctx, cursor, limit, false)
}

func (s *Image) ListTrash(ctx context.Context, cursor *model.ImageCursor, limit int) (ImagePage, error) {
	return s.list(ctx, cursor, limit, true)
}

func (s *Image) list(ctx context.Context, cursor *model.ImageCursor, limit int, deleted bool) (ImagePage, error) {
	limit = clampLimit(limit)
	// 多取一条只用于判断是否存在下一页，不会返回给调用方。
	var images []model.Image
	var err error
	if deleted {
		images, err = s.listDeleted(ctx, cursor, int64(limit+1))
	} else {
		images, err = s.listActive(ctx, cursor, int64(limit+1))
	}
	if err != nil {
		return ImagePage{}, fmt.Errorf("查询图片列表: %w", err)
	}
	page := ImagePage{Images: images}
	if len(images) > limit {
		// 游标取当前页最后一条记录的排序字段，下一页从它之后继续查询。
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
	// 软删除只更新数据库，原图和缩略图继续保留，以便从回收站恢复。
	now := s.now().Unix()
	rows, err := s.queries.SoftDeleteImage(ctx, repo.SoftDeleteImageParams{
		DeletedAt: &now, UpdatedAt: now, PublicID: publicID,
	})
	if err != nil {
		return fmt.Errorf("软删除图片: %w", err)
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
	// 如果删除后又上传了相同像素的图片，恢复会违反有效图片唯一索引。
	if existing, findErr := s.findActiveByPixelHash(ctx, deleted.PixelHash); findErr == nil {
		return model.Image{}, &RestoreConflictError{ExistingPublicID: existing.PublicID}
	} else if !errors.Is(findErr, ErrNotFound) {
		return model.Image{}, fmt.Errorf("检查图片恢复冲突: %w", findErr)
	}

	rows, err := s.queries.RestoreImage(ctx, repo.RestoreImageParams{UpdatedAt: s.now().Unix(), PublicID: publicID})
	if err != nil {
		// 并发请求可能在前一次检查后插入重复图片，因此数据库报错后再查一次冲突对象。
		if existing, findErr := s.findActiveByPixelHash(ctx, deleted.PixelHash); findErr == nil {
			return model.Image{}, &RestoreConflictError{ExistingPublicID: existing.PublicID}
		}
		return model.Image{}, fmt.Errorf("恢复图片: %w", err)
	}
	if rows == 0 {
		return model.Image{}, ErrNotFound
	}
	return s.Get(ctx, publicID)
}

func (s *Image) DeletePermanently(ctx context.Context, publicID string) (model.Image, error) {
	// 先用 DELETE RETURNING 取得存储 key，再清理对应文件。
	// 若文件删除失败，数据库记录已经不存在，后续可由孤立文件清理任务重试。
	row, err := s.queries.DeleteImagePermanently(ctx, publicID)
	if err != nil {
		return model.Image{}, mapNotFound(err)
	}
	image := imageFromRow(row)
	if err := s.storage.RemoveImage(image.StorageKey, image.ThumbnailKey); err != nil {
		return image, fmt.Errorf("删除已存储图片: %w", err)
	}
	return image, nil
}

func validateOriginalName(name string) error {
	if strings.TrimSpace(name) == "" || utf8.RuneCountInString(name) > 255 {
		return fmt.Errorf("%w: 原始文件名长度必须为 1 到 255 个字符", ErrInvalidInput)
	}
	return nil
}

const (
	imageIDLength   = 12
	imageIDAlphabet = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz"
	// 248 是不超过 256 的最大 62 的倍数；丢弃更大的字节可避免取模偏差。
	imageIDByteLimit = 256 - 256%len(imageIDAlphabet)
)

func randomImageID(reader io.Reader) (string, error) {
	result := make([]byte, 0, imageIDLength)
	for len(result) < imageIDLength {
		buffer := make([]byte, imageIDLength-len(result))
		if _, err := io.ReadFull(reader, buffer); err != nil {
			return "", err
		}
		for _, value := range buffer {
			if int(value) >= imageIDByteLimit {
				continue
			}
			result = append(result, imageIDAlphabet[int(value)%len(imageIDAlphabet)])
		}
	}
	return string(result), nil
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
