package image

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"errors"
	"fmt"
	img "image"
	_ "image/jpeg"
	_ "image/png"
	"io"

	"github.com/deepteams/webp"
	"github.com/disintegration/imaging"
	"github.com/lemonc7/lensy/config"
)

var (
	ErrTooLarge          = errors.New("上传图片超过大小限制")
	ErrTooManyPixels     = errors.New("图片像素数超过限制")
	ErrUnsupportedFormat = errors.New("不支持的图片格式")
	ErrAnimatedWebP      = errors.New("暂不支持动态 WebP")
	ErrInvalidImage      = errors.New("图片内容无效或已损坏")
)

type Processor struct {
	config config.ImageConfig
}

// Result 同时包含正式 WebP、缩略图以及写入数据库所需的元数据。
type Result struct {
	WebP          []byte
	ThumbnailWebP []byte

	Width           int64
	Height          int64
	ThumbnailWidth  int64
	ThumbnailHeight int64

	ContentHash string // 最终 WebP 字节哈希，用于校验磁盘文件完整性
	PixelHash   string // 方向归一化后的像素哈希，用于图片查重
}

func NewProcessor(cfg config.ImageConfig) (*Processor, error) {
	if err := cfg.Validate(); err != nil {
		return nil, err
	}
	return &Processor{config: cfg}, nil
}

func (p *Processor) Process(source io.Reader) (Result, error) {
	// 多读一个字节即可判断是否超限，避免无上限地把请求内容读入内存。
	data, err := io.ReadAll(io.LimitReader(source, p.config.MaxUploadSize+1))
	if err != nil {
		return Result{}, fmt.Errorf("读取上传图片: %w", err)
	}
	if int64(len(data)) > p.config.MaxUploadSize {
		return Result{}, ErrTooLarge
	}

	// 完整解码前先读取尺寸，防止尺寸极大的压缩图片耗尽内存。
	imageConfig, format, err := img.DecodeConfig(bytes.NewReader(data))
	if err != nil {
		return Result{}, fmt.Errorf("%w: 读取图片信息失败: %v", ErrInvalidImage, err)
	}
	if err := validateFormat(format); err != nil {
		return Result{}, err
	}
	if imageConfig.Width <= 0 || imageConfig.Height <= 0 {
		return Result{}, errors.New("图片尺寸无效")
	}
	width := int64(imageConfig.Width)
	height := int64(imageConfig.Height)
	// 使用除法比较，避免恶意构造的超大尺寸在相乘时发生 int64 溢出。
	if width > p.config.MaxPixels/height {
		return Result{}, ErrTooManyPixels
	}

	if format == "webp" {
		// 当前只处理静态图片，避免上传动态 WebP 后悄悄只保留其中一帧。
		features, err := webp.GetFeatures(bytes.NewReader(data))
		if err != nil {
			return Result{}, fmt.Errorf("%w: 读取 WebP 信息失败: %v", ErrInvalidImage, err)
		}
		if features.HasAnimation {
			return Result{}, ErrAnimatedWebP
		}
	}

	// 先应用 JPEG EXIF Orientation，再计算尺寸、缩略图和 PixelHash。
	// 这样手机照片的显示方向会直接写进像素，输出 WebP 无需保留方向元数据。
	decoded, err := imaging.Decode(bytes.NewReader(data), imaging.AutoOrientation(true))
	if err != nil {
		return Result{}, fmt.Errorf("%w: 解码图片失败: %v", ErrInvalidImage, err)
	}
	normalized := imaging.Clone(decoded)
	thumbnail := normalized
	// 小图不放大；大图按最长边等比缩小，不做裁切。
	if normalized.Bounds().Dx() > p.config.ThumbnailMaxEdge ||
		normalized.Bounds().Dy() > p.config.ThumbnailMaxEdge {
		thumbnail = imaging.Fit(
			normalized,
			p.config.ThumbnailMaxEdge,
			p.config.ThumbnailMaxEdge,
			imaging.Lanczos,
		)
	}

	encoded, err := encodeWebP(normalized, p.config.Quality, p.config.Method)
	if err != nil {
		return Result{}, fmt.Errorf("编码 WebP: %w", err)
	}
	encodedThumbnail, err := encodeWebP(thumbnail, p.config.ThumbnailQuality, p.config.Method)
	if err != nil {
		return Result{}, fmt.Errorf("编码 WebP 缩略图: %w", err)
	}

	bounds := normalized.Bounds()
	thumbnailBounds := thumbnail.Bounds()
	return Result{
		WebP: encoded, ThumbnailWebP: encodedThumbnail,
		Width: int64(bounds.Dx()), Height: int64(bounds.Dy()),
		ThumbnailWidth: int64(thumbnailBounds.Dx()), ThumbnailHeight: int64(thumbnailBounds.Dy()),
		ContentHash: hash(encoded), PixelHash: pixelHash(normalized),
	}, nil
}

func validateFormat(format string) error {
	switch format {
	case "jpeg", "png", "webp":
		return nil
	default:
		return fmt.Errorf("%w: %s", ErrUnsupportedFormat, format)
	}
}

func encodeWebP(source img.Image, quality float32, method int) ([]byte, error) {
	var output bytes.Buffer
	// Photo 预设面向照片类内容，SharpYUV 能改善色彩边缘和文字附近的色度质量。
	options := webp.OptionsForPreset(webp.PresetPhoto, quality)
	options.Method = method
	options.UseSharpYUV = true
	if err := webp.Encode(&output, source, options); err != nil {
		return nil, err
	}
	return output.Bytes(), nil
}

func hash(data []byte) string {
	sum := sha256.Sum256(data)
	return fmt.Sprintf("%x", sum)
}

func pixelHash(source *img.NRGBA) string {
	hasher := sha256.New()
	// 把尺寸也加入哈希，避免不同宽高的图片仅凭像素字节产生歧义。
	_ = binary.Write(hasher, binary.LittleEndian, uint32(source.Bounds().Dx()))
	_ = binary.Write(hasher, binary.LittleEndian, uint32(source.Bounds().Dy()))
	_, _ = hasher.Write(source.Pix)
	return fmt.Sprintf("%x", hasher.Sum(nil))
}
