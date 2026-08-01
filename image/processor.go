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
)

type Processor struct {
	config config.ImageConfig
}

type Result struct {
	WebP          []byte
	ThumbnailWebP []byte

	Width           int64
	Height          int64
	ThumbnailWidth  int64
	ThumbnailHeight int64

	ContentHash string
	PixelHash   string
}

func NewProcessor(cfg config.ImageConfig) (*Processor, error) {
	if err := cfg.Validate(); err != nil {
		return nil, err
	}
	return &Processor{config: cfg}, nil
}

func (p *Processor) Process(source io.Reader) (Result, error) {
	data, err := io.ReadAll(io.LimitReader(source, p.config.MaxUploadSize+1))
	if err != nil {
		return Result{}, fmt.Errorf("读取上传图片: %w", err)
	}
	if int64(len(data)) > p.config.MaxUploadSize {
		return Result{}, ErrTooLarge
	}

	imageConfig, format, err := img.DecodeConfig(bytes.NewReader(data))
	if err != nil {
		return Result{}, fmt.Errorf("读取图片信息: %w", err)
	}
	if err := validateFormat(format); err != nil {
		return Result{}, err
	}
	if imageConfig.Width <= 0 || imageConfig.Height <= 0 {
		return Result{}, errors.New("图片尺寸无效")
	}
	pixels := int64(imageConfig.Width) * int64(imageConfig.Height)
	if pixels > p.config.MaxPixels {
		return Result{}, ErrTooManyPixels
	}

	if format == "webp" {
		features, err := webp.GetFeatures(bytes.NewReader(data))
		if err != nil {
			return Result{}, fmt.Errorf("读取 WebP 信息: %w", err)
		}
		if features.HasAnimation {
			return Result{}, ErrAnimatedWebP
		}
	}

	decoded, err := imaging.Decode(bytes.NewReader(data), imaging.AutoOrientation(true))
	if err != nil {
		return Result{}, fmt.Errorf("解码图片: %w", err)
	}
	normalized := imaging.Clone(decoded)
	thumbnail := normalized
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
	_ = binary.Write(hasher, binary.LittleEndian, uint32(source.Bounds().Dx()))
	_ = binary.Write(hasher, binary.LittleEndian, uint32(source.Bounds().Dy()))
	_, _ = hasher.Write(source.Pix)
	return fmt.Sprintf("%x", hasher.Sum(nil))
}
