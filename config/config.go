package config

import (
	"errors"
	"fmt"
	"net"
	"net/url"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

// Config 是应用的顶层配置，所有模块配置统一从这里进入。
type Config struct {
	Server   ServerConfig
	Database DatabaseConfig
	Image    ImageConfig
}

// Validate 校验所有模块配置。
func (c Config) Validate() error {
	if err := c.Server.Validate(); err != nil {
		return fmt.Errorf("服务器配置无效: %w", err)
	}
	if err := c.Database.Validate(); err != nil {
		return fmt.Errorf("数据库配置无效: %w", err)
	}
	if err := c.Image.Validate(); err != nil {
		return fmt.Errorf("图片配置无效: %w", err)
	}
	return nil
}

// ImageConfig 定义上传限制、WebP 编码参数和缩略图尺寸。
type ImageConfig struct {
	MaxUploadSize    int64
	MaxPixels        int64
	Quality          float32
	ThumbnailQuality float32
	Method           int
	ThumbnailMaxEdge int
}

// Validate 校验图片处理配置。
func (c ImageConfig) Validate() error {
	if c.MaxUploadSize <= 0 {
		return errors.New("最大上传大小必须为正数")
	}
	if c.MaxPixels <= 0 {
		return errors.New("最大像素数必须为正数")
	}
	if c.Quality < 0 || c.Quality > 100 {
		return errors.New("WebP 质量必须在 0 到 100 之间")
	}
	if c.ThumbnailQuality < 0 || c.ThumbnailQuality > 100 {
		return errors.New("缩略图 WebP 质量必须在 0 到 100 之间")
	}
	if c.Method < 0 || c.Method > 6 {
		return errors.New("WebP 编码强度必须在 0 到 6 之间")
	}
	if c.ThumbnailMaxEdge <= 0 {
		return errors.New("缩略图最长边必须为正数")
	}
	return nil
}

// ServerConfig 定义 HTTP 服务的监听地址、超时和请求头限制。
type ServerConfig struct {
	Host              string
	Port              int
	ReadHeaderTimeout time.Duration
	ReadTimeout       time.Duration
	WriteTimeout      time.Duration
	IdleTimeout       time.Duration
	ShutdownTimeout   time.Duration
	MaxHeaderBytes    int
}

// Address 返回可直接传给 http.Server.Addr 的监听地址。
func (c ServerConfig) Address() string {
	return net.JoinHostPort(c.Host, strconv.Itoa(c.Port))
}

// Validate 校验服务器配置。
func (c ServerConfig) Validate() error {
	if strings.TrimSpace(c.Host) == "" {
		return errors.New("监听主机不能为空")
	}
	if c.Port < 1 || c.Port > 65535 {
		return errors.New("监听端口必须在 1 到 65535 之间")
	}
	if c.ReadHeaderTimeout <= 0 {
		return errors.New("请求头读取超时必须为正数")
	}
	if c.ReadTimeout <= 0 {
		return errors.New("请求读取超时必须为正数")
	}
	if c.WriteTimeout <= 0 {
		return errors.New("响应写入超时必须为正数")
	}
	if c.IdleTimeout <= 0 {
		return errors.New("空闲连接超时必须为正数")
	}
	if c.ShutdownTimeout <= 0 {
		return errors.New("优雅关闭超时必须为正数")
	}
	if c.MaxHeaderBytes <= 0 {
		return errors.New("最大请求头大小必须为正数")
	}
	return nil
}

// DatabaseConfig 定义 SQLite 连接池和锁等待配置。
// 外键、WAL 等数据库约束固定写入 DSN，不允许通过配置关闭。
type DatabaseConfig struct {
	MaxOpenConns    int
	MaxIdleConns    int
	ConnMaxLifetime time.Duration
	ConnMaxIdleTime time.Duration
	BusyTimeout     time.Duration
}

// Validate 校验数据库配置。
func (c DatabaseConfig) Validate() error {
	if c.MaxOpenConns <= 0 {
		return errors.New("最大连接数必须为正数")
	}
	if c.MaxIdleConns < 0 {
		return errors.New("最大空闲连接数不能为负数")
	}
	if c.MaxIdleConns > c.MaxOpenConns {
		return errors.New("最大空闲连接数不能超过最大连接数")
	}
	if c.ConnMaxLifetime < 0 {
		return errors.New("连接最大存活时间不能为负数")
	}
	if c.ConnMaxIdleTime < 0 {
		return errors.New("连接最大空闲时间不能为负数")
	}
	if c.BusyTimeout <= 0 {
		return errors.New("数据库锁等待时间必须为正数")
	}
	return nil
}

// DSN 返回 modernc.org/sqlite 使用的连接字符串。
// PRAGMA 写入 DSN 后会应用到 database/sql 创建的每一条连接。
func (c DatabaseConfig) DSN(path string) (string, error) {
	if err := c.Validate(); err != nil {
		return "", err
	}

	busyMilliseconds := c.BusyTimeout.Milliseconds()
	if busyMilliseconds < 1 {
		return "", fmt.Errorf("数据库锁等待时间不能小于 %s", time.Millisecond)
	}

	if path == ":memory:" {
		query := url.Values{
			"mode":    {"memory"},
			"cache":   {"shared"},
			"_pragma": pragmaValues(busyMilliseconds),
		}
		return "file:lensy?" + query.Encode(), nil
	}

	u := &url.URL{Scheme: "file", Path: filepath.ToSlash(path)}
	query := u.Query()
	for _, pragma := range pragmaValues(busyMilliseconds) {
		query.Add("_pragma", pragma)
	}
	u.RawQuery = query.Encode()
	return u.String(), nil
}

func pragmaValues(busyMilliseconds int64) []string {
	return []string{
		"foreign_keys(1)",
		"journal_mode(WAL)",
		"synchronous(NORMAL)",
		"busy_timeout(" + strconv.FormatInt(busyMilliseconds, 10) + ")",
	}
}
