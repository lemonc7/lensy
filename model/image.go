package model

type Image struct {
	ID              int64  `json:"id"`
	PublicID        string `json:"public_id"`
	StorageKey      string `json:"storage_key"`
	ThumbnailKey    string `json:"thumbnail_key"`
	OriginalName    string `json:"original_name"`
	StoredSize      int64  `json:"stored_size"`
	ThumbnailSize   int64  `json:"thumbnail_size"`
	Width           int64  `json:"width"`
	Height          int64  `json:"height"`
	ThumbnailWidth  int64  `json:"thumbnail_width"`
	ThumbnailHeight int64  `json:"thumbnail_height"`
	ContentHash     string `json:"content_hash"`
	PixelHash       string `json:"pixel_hash"`
	CreatedAt       int64  `json:"created_at"`
	UpdatedAt       int64  `json:"updated_at"`
	DeletedAt       *int64 `json:"deleted_at,omitempty"`
}

type ImageCursor struct {
	Timestamp int64 `json:"timestamp"`
	ID        int64 `json:"id"`
}
