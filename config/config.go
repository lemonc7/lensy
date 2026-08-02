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

	"github.com/ilyakaznacheev/cleanenv"
)

const defaultConfigPath = "config/config.yml"

// Config 是应用的顶层配置，所有模块配置统一从这里进入。
type Config struct {
	Server   ServerConfig   `yaml:"server"`
	Database DatabaseConfig `yaml:"database"`
	Image    ImageConfig    `yaml:"image"`
	Auth     AuthConfig     `yaml:"auth"`
}

// Load 从固定的 YAML 文件读取配置，再使用环境变量覆盖对应字段。
func Load() (Config, error) {
	var cfg Config
	if err := cleanenv.ReadConfig(defaultConfigPath, &cfg); err != nil {
		return Config{}, fmt.Errorf("读取配置文件 %q: %w", defaultConfigPath, err)
	}

	if err := cfg.Validate(); err != nil {
		return Config{}, fmt.Errorf("校验配置文件 %q: %w", defaultConfigPath, err)
	}
	return cfg, nil
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
	if err := c.Auth.Validate(); err != nil {
		return fmt.Errorf("认证配置无效: %w", err)
	}
	return nil
}

// AuthConfig 定义网页登录使用的加密 Cookie Session 参数。
type AuthConfig struct {
	SessionAuthKey       string        `yaml:"session_auth_key" env:"LENSY_SESSION_AUTH_KEY"`
	SessionEncryptionKey string        `yaml:"session_encryption_key" env:"LENSY_SESSION_ENCRYPTION_KEY"`
	SessionTTL           time.Duration `yaml:"session_ttl" env:"LENSY_SESSION_TTL" env-default:"168h"`
	SecureCookie         bool          `yaml:"secure_cookie" env:"LENSY_SECURE_COOKIE" env-default:"false"`
}

// Validate 校验 Session 签名、加密和有效期配置。
func (c AuthConfig) Validate() error {
	if len(c.SessionAuthKey) < 32 {
		return errors.New("Session 签名密钥不能少于 32 字节")
	}
	switch len(c.SessionEncryptionKey) {
	case 16, 24, 32:
	default:
		return errors.New("Session 加密密钥长度必须为 16、24 或 32 字节")
	}
	if c.SessionTTL < time.Second {
		return errors.New("Session 有效期不能小于 1 秒")
	}
	return nil
}

// ImageConfig 定义上传限制、WebP 编码参数和缩略图尺寸。
type ImageConfig struct {
	MaxUploadSize    int64   `yaml:"max_upload_size" env:"LENSY_IMAGE_MAX_UPLOAD_SIZE" env-default:"20971520"`
	MaxPixels        int64   `yaml:"max_pixels" env:"LENSY_IMAGE_MAX_PIXELS" env-default:"40000000"`
	Quality          float32 `yaml:"quality" env:"LENSY_IMAGE_QUALITY" env-default:"82"`
	ThumbnailQuality float32 `yaml:"thumbnail_quality" env:"LENSY_IMAGE_THUMBNAIL_QUALITY" env-default:"75"`
	Method           int     `yaml:"method" env:"LENSY_IMAGE_METHOD" env-default:"4"`
	ThumbnailMaxEdge int     `yaml:"thumbnail_max_edge" env:"LENSY_IMAGE_THUMBNAIL_MAX_EDGE" env-default:"480"`
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
	Host              string        `yaml:"host" env:"LENSY_HOST" env-default:"127.0.0.1"`
	Port              int           `yaml:"port" env:"LENSY_PORT" env-default:"8080"`
	PublicURL         string        `yaml:"public_url" env:"LENSY_PUBLIC_URL"`
	ReadHeaderTimeout time.Duration `yaml:"read_header_timeout" env:"LENSY_READ_HEADER_TIMEOUT" env-default:"5s"`
	ReadTimeout       time.Duration `yaml:"read_timeout" env:"LENSY_READ_TIMEOUT" env-default:"30s"`
	WriteTimeout      time.Duration `yaml:"write_timeout" env:"LENSY_WRITE_TIMEOUT" env-default:"60s"`
	IdleTimeout       time.Duration `yaml:"idle_timeout" env:"LENSY_IDLE_TIMEOUT" env-default:"2m"`
	ShutdownTimeout   time.Duration `yaml:"shutdown_timeout" env:"LENSY_SHUTDOWN_TIMEOUT" env-default:"15s"`
	MaxHeaderBytes    int           `yaml:"max_header_bytes" env:"LENSY_MAX_HEADER_BYTES" env-default:"1048576"`
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
	if publicURL := strings.TrimSpace(c.PublicURL); publicURL != "" {
		parsed, err := url.Parse(publicURL)
		if err != nil || parsed.Host == "" || (parsed.Scheme != "http" && parsed.Scheme != "https") {
			return errors.New("公开访问地址必须是完整的 HTTP 或 HTTPS 地址")
		}
		if parsed.User != nil || parsed.RawQuery != "" || parsed.Fragment != "" {
			return errors.New("公开访问地址不能包含用户信息、查询参数或片段")
		}
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
	MaxOpenConns    int           `yaml:"max_open_conns" env:"LENSY_DB_MAX_OPEN_CONNS" env-default:"4"`
	MaxIdleConns    int           `yaml:"max_idle_conns" env:"LENSY_DB_MAX_IDLE_CONNS" env-default:"4"`
	ConnMaxLifetime time.Duration `yaml:"conn_max_lifetime" env:"LENSY_DB_CONN_MAX_LIFETIME" env-default:"0s"`
	ConnMaxIdleTime time.Duration `yaml:"conn_max_idle_time" env:"LENSY_DB_CONN_MAX_IDLE_TIME" env-default:"0s"`
	BusyTimeout     time.Duration `yaml:"busy_timeout" env:"LENSY_DB_BUSY_TIMEOUT" env-default:"5s"`
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
