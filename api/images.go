package api

import (
	"encoding/base64"
	"errors"
	"fmt"
	"net/http"
	"path"
	"strconv"
	"strings"
	"time"

	"github.com/labstack/echo/v5"
	imageprocessor "github.com/lemonc7/lensy/image"
	"github.com/lemonc7/lensy/model"
	"github.com/lemonc7/lensy/service"
)

type Images struct {
	service   *service.Image
	publicURL string
}

func NewImages(imageService *service.Image, publicURL string) (*Images, error) {
	if imageService == nil {
		return nil, errors.New("图片服务不能为空")
	}
	return &Images{
		service:   imageService,
		publicURL: strings.TrimRight(strings.TrimSpace(publicURL), "/"),
	}, nil
}

type imageUploadResponse struct {
	service.UploadImageResult
	URL          string `json:"url"`
	ThumbnailURL string `json:"thumbnail_url"`
}

type picGoUploadData struct {
	URL           string `json:"url"`
	ThumbnailURL  string `json:"thumbnail_url"`
	PublicID      string `json:"public_id"`
	AlreadyExists bool   `json:"already_exists"`
}

// picGoUploadResponse 同时兼容 PicGo 官方上传流程和常见的 Web Uploader 插件字段。
type picGoUploadResponse struct {
	Success bool            `json:"success"`
	Result  []string        `json:"result"`
	URL     string          `json:"url"`
	ImgURL  string          `json:"imgUrl"`
	Data    picGoUploadData `json:"data"`
}

// Upload 接收 multipart/form-data 中名为 file 的单张图片。
func (h *Images) Upload(c *echo.Context) error {
	result, err := h.upload(c)
	if err != nil {
		return err
	}

	status := http.StatusCreated
	if result.AlreadyExists {
		status = http.StatusOK
	}
	return c.JSON(status, imageUploadResponse{
		UploadImageResult: result,
		URL:               h.imageURL(c, result.Image, false),
		ThumbnailURL:      h.imageURL(c, result.Image, true),
	})
}

// UploadForPicGo 使用 Bearer Token 鉴权，返回 PicGo 容易直接提取的绝对图片地址。
func (h *Images) UploadForPicGo(c *echo.Context) error {
	result, err := h.upload(c)
	if err != nil {
		return err
	}
	imageURL := h.imageURL(c, result.Image, false)
	return c.JSON(http.StatusOK, picGoUploadResponse{
		Success: true,
		Result:  []string{imageURL},
		URL:     imageURL,
		ImgURL:  imageURL,
		Data: picGoUploadData{
			URL:           imageURL,
			ThumbnailURL:  h.imageURL(c, result.Image, true),
			PublicID:      result.Image.PublicID,
			AlreadyExists: result.AlreadyExists,
		},
	})
}

func (h *Images) upload(c *echo.Context) (service.UploadImageResult, error) {
	fileHeader, err := c.FormFile("file")
	if errors.Is(err, http.ErrMissingFile) {
		// 部分 PicGo 上传器默认使用 files 作为字段名。
		fileHeader, err = c.FormFile("files")
	}
	// 即使 multipart 解析中途失败，也清理已经写入磁盘的临时部分。
	if form := c.Request().MultipartForm; form != nil {
		defer form.RemoveAll()
	}
	if err != nil {
		if errors.Is(err, echo.ErrStatusRequestEntityTooLarge) {
			return service.UploadImageResult{}, echo.NewHTTPError(http.StatusRequestEntityTooLarge, "上传内容超过大小限制")
		}
		return service.UploadImageResult{}, echo.NewHTTPError(http.StatusBadRequest, "缺少上传图片，请使用 file 或 files 字段").Wrap(err)
	}

	file, err := fileHeader.Open()
	if err != nil {
		return service.UploadImageResult{}, echo.NewHTTPError(http.StatusBadRequest, "打开上传图片失败").Wrap(err)
	}
	defer file.Close()

	result, err := h.service.Upload(c.Request().Context(), service.UploadImageInput{
		OriginalName: fileHeader.Filename,
		Source:       file,
	})
	if err != nil {
		return service.UploadImageResult{}, imageHTTPError(err)
	}
	return result, nil
}

func (h *Images) Get(c *echo.Context) error {
	publicID, err := imagePublicID(c)
	if err != nil {
		return err
	}
	storedImage, err := h.service.Get(c.Request().Context(), publicID)
	if err != nil {
		return imageHTTPError(err)
	}
	return c.JSON(http.StatusOK, storedImage)
}

func (h *Images) List(c *echo.Context) error {
	cursor, limit, err := imagePageParams(c)
	if err != nil {
		return err
	}
	page, err := h.service.List(c.Request().Context(), cursor, limit)
	if err != nil {
		return imageHTTPError(err)
	}
	return c.JSON(http.StatusOK, page)
}

func (h *Images) ListTrash(c *echo.Context) error {
	cursor, limit, err := imagePageParams(c)
	if err != nil {
		return err
	}
	page, err := h.service.ListTrash(c.Request().Context(), cursor, limit)
	if err != nil {
		return imageHTTPError(err)
	}
	return c.JSON(http.StatusOK, page)
}

func (h *Images) SoftDelete(c *echo.Context) error {
	publicID, err := imagePublicID(c)
	if err != nil {
		return err
	}
	if err := h.service.SoftDelete(c.Request().Context(), publicID); err != nil {
		return imageHTTPError(err)
	}
	return c.NoContent(http.StatusNoContent)
}

func (h *Images) Restore(c *echo.Context) error {
	publicID, err := imagePublicID(c)
	if err != nil {
		return err
	}
	storedImage, err := h.service.Restore(c.Request().Context(), publicID)
	if err != nil {
		return imageHTTPError(err)
	}
	return c.JSON(http.StatusOK, storedImage)
}

func (h *Images) DeletePermanently(c *echo.Context) error {
	publicID, err := imagePublicID(c)
	if err != nil {
		return err
	}
	if _, err := h.service.DeletePermanently(c.Request().Context(), publicID); err != nil {
		return imageHTTPError(err)
	}
	return c.NoContent(http.StatusNoContent)
}

func (h *Images) ServeImage(c *echo.Context) error {
	return h.serveFile(c, false)
}

func (h *Images) ServeThumbnail(c *echo.Context) error {
	return h.serveFile(c, true)
}

func (h *Images) serveFile(c *echo.Context, thumbnail bool) error {
	publicID, requestedPath, err := publicImageRequest(c)
	if err != nil {
		return err
	}
	storedImage, file, err := h.service.OpenFile(c.Request().Context(), publicID, thumbnail)
	if err != nil {
		return imageHTTPError(err)
	}
	defer file.Close()
	expectedPath := storedImage.StorageKey
	prefix := "images/"
	if thumbnail {
		expectedPath = storedImage.ThumbnailKey
		prefix = "thumbnails/"
	}
	if requestedPath != strings.TrimPrefix(expectedPath, prefix) {
		return echo.NewHTTPError(http.StatusNotFound, "图片不存在")
	}

	info, err := file.Stat()
	if err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, "读取图片文件信息失败").Wrap(err)
	}

	etag := storedImage.ContentHash
	if thumbnail {
		etag = storedImage.PublicID + "-thumbnail"
	}
	header := c.Response().Header()
	header.Set(echo.HeaderContentType, "image/webp")
	header.Set(echo.HeaderCacheControl, "public, max-age=300")
	header.Set("ETag", strconv.Quote(etag))
	header.Set(echo.HeaderContentDisposition, fmt.Sprintf(`inline; filename="%s.webp"`, storedImage.PublicID))
	http.ServeContent(c.Response(), c.Request(), storedImage.PublicID+".webp", info.ModTime(), file)
	return nil
}

func (h *Images) imageURL(c *echo.Context, storedImage model.Image, thumbnail bool) string {
	baseURL := h.publicURL
	if baseURL == "" {
		baseURL = c.Scheme() + "://" + c.Request().Host
	}
	prefix := "images"
	key := storedImage.StorageKey
	if thumbnail {
		prefix = "thumbnails"
		key = storedImage.ThumbnailKey
	}
	relativePath := strings.TrimPrefix(key, prefix+"/")
	return baseURL + "/" + prefix + "/" + relativePath
}

func imagePublicID(c *echo.Context) (string, error) {
	publicID, ok := normalizeImagePublicID(c.Param("public_id"))
	if !ok {
		return "", echo.NewHTTPError(http.StatusBadRequest, "图片 ID 格式错误")
	}
	return publicID, nil
}

func publicImageRequest(c *echo.Context) (publicID string, requestedPath string, err error) {
	requestedPath = strings.Trim(c.Param("*"), "/")
	if !isDatedImagePath(requestedPath) {
		return "", "", echo.NewHTTPError(http.StatusNotFound, "图片不存在")
	}
	filename := path.Base(requestedPath)
	publicID, ok := normalizeImagePublicID(strings.TrimSuffix(filename, ".webp"))
	if !ok {
		return "", "", echo.NewHTTPError(http.StatusNotFound, "图片不存在")
	}
	return publicID, requestedPath, nil
}

func isDatedImagePath(value string) bool {
	if path.Clean(value) != value {
		return false
	}
	parts := strings.Split(value, "/")
	if len(parts) != 3 && len(parts) != 4 {
		return false
	}
	if !strings.HasSuffix(parts[len(parts)-1], ".webp") {
		return false
	}
	dateParts := parts[:len(parts)-1]
	dateFormat := "2006/01"
	if len(dateParts) == 3 {
		// 兼容改为按月分目录前已经生成的公开链接。
		dateFormat = "2006/01/02"
	}
	_, err := time.Parse(dateFormat, strings.Join(dateParts, "/"))
	return err == nil
}

func normalizeImagePublicID(value string) (string, bool) {
	value = strings.TrimSpace(value)
	if len(value) != 12 {
		return "", false
	}
	decoded, err := base64.RawURLEncoding.DecodeString(value)
	return value, err == nil && len(decoded) == 9
}

func imagePageParams(c *echo.Context) (*model.ImageCursor, int, error) {
	limit := 20
	if raw := strings.TrimSpace(c.QueryParam("limit")); raw != "" {
		value, err := strconv.Atoi(raw)
		if err != nil || value < 1 || value > 100 {
			return nil, 0, echo.NewHTTPError(http.StatusBadRequest, "limit 必须是 1 到 100 之间的整数")
		}
		limit = value
	}

	rawTimestamp := strings.TrimSpace(c.QueryParam("cursor_timestamp"))
	rawID := strings.TrimSpace(c.QueryParam("cursor_id"))
	if rawTimestamp == "" && rawID == "" {
		return nil, limit, nil
	}
	if rawTimestamp == "" || rawID == "" {
		return nil, 0, echo.NewHTTPError(http.StatusBadRequest, "cursor_timestamp 和 cursor_id 必须同时提供")
	}

	timestamp, timestampErr := strconv.ParseInt(rawTimestamp, 10, 64)
	id, idErr := strconv.ParseInt(rawID, 10, 64)
	if timestampErr != nil || idErr != nil || timestamp < 0 || id <= 0 {
		return nil, 0, echo.NewHTTPError(http.StatusBadRequest, "分页游标格式错误")
	}
	return &model.ImageCursor{Timestamp: timestamp, ID: id}, limit, nil
}

func imageHTTPError(err error) error {
	var conflict *service.RestoreConflictError
	switch {
	case errors.Is(err, service.ErrNotFound):
		return echo.NewHTTPError(http.StatusNotFound, "图片不存在")
	case errors.As(err, &conflict):
		return echo.NewHTTPError(
			http.StatusConflict,
			"已存在像素内容相同的有效图片，图片 ID 为 "+conflict.ExistingPublicID,
		)
	case errors.Is(err, service.ErrInvalidInput):
		return echo.NewHTTPError(http.StatusBadRequest, "图片参数无效").Wrap(err)
	case errors.Is(err, imageprocessor.ErrTooLarge):
		return echo.NewHTTPError(http.StatusRequestEntityTooLarge, "上传图片超过大小限制")
	case errors.Is(err, imageprocessor.ErrTooManyPixels):
		return echo.NewHTTPError(http.StatusUnprocessableEntity, "图片像素数超过限制")
	case errors.Is(err, imageprocessor.ErrUnsupportedFormat):
		return echo.NewHTTPError(http.StatusUnsupportedMediaType, "仅支持 JPEG、PNG 和 WebP 图片")
	case errors.Is(err, imageprocessor.ErrAnimatedWebP):
		return echo.NewHTTPError(http.StatusUnsupportedMediaType, "暂不支持动态 WebP")
	case errors.Is(err, imageprocessor.ErrInvalidImage):
		return echo.NewHTTPError(http.StatusBadRequest, "图片内容无效或已损坏")
	default:
		return echo.NewHTTPError(http.StatusInternalServerError, "服务器内部错误").Wrap(err)
	}
}
