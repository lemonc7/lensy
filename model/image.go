package model

type Image struct {
	ID              int64  `json:"id"`
	PublicID        string `json:"public_id"`   // 对外使用的随机标识，不暴露数据库自增 ID
	StorageKey      string `json:"storage_key"` // 相对于 data 目录的正式图片路径
	ThumbnailKey    string `json:"thumbnail_key"`
	OriginalName    string `json:"original_name"`
	StoredSize      int64  `json:"stored_size"`
	ThumbnailSize   int64  `json:"thumbnail_size"`
	Width           int64  `json:"width"`
	Height          int64  `json:"height"`
	ThumbnailWidth  int64  `json:"thumbnail_width"`
	ThumbnailHeight int64  `json:"thumbnail_height"`
	ContentHash     string `json:"content_hash"` // 最终 WebP 完整性校验
	PixelHash       string `json:"pixel_hash"`   // 标准化像素查重
	CreatedAt       int64  `json:"created_at"`
	UpdatedAt       int64  `json:"updated_at"`
	DeletedAt       *int64 `json:"deleted_at,omitempty"`
}

type ImageCursor struct {
	Timestamp int64 `json:"timestamp"` // 有效列表使用 created_at，回收站使用 deleted_at
	ID        int64 `json:"id"`        // 时间相同时用 ID 保证排序和翻页稳定
}
