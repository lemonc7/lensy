package app

import (
	"errors"
	"net/http"
	"time"

	"github.com/labstack/echo/v5"
	"github.com/labstack/echo/v5/middleware"
	"github.com/lemonc7/lensy/api"
	"github.com/lemonc7/lensy/config"
)

// NewRouter 创建 Echo 实例并注册认证、Token、图片管理和图床上传路由。
func NewRouter(auth *api.Auth, images *api.Images, tokens *api.APITokens, cfg config.Config) *echo.Echo {
	e := echo.New()
	e.Use(middleware.RequestLogger())
	e.Use(middleware.Recover())
	e.Use(middleware.Secure())
	e.Use(middleware.CSRFWithConfig(middleware.CSRFConfig{
		// PicGo 不使用浏览器 Cookie，独立上传接口改由 Bearer Token 防护。
		Skipper: func(c *echo.Context) bool {
			return c.Request().URL.Path == "/api/upload"
		},
		TokenLookup:    "header:X-CSRF-Token",
		ContextKey:     "csrf",
		CookieName:     "lensy_csrf",
		CookiePath:     "/",
		CookieMaxAge:   86400,
		CookieSecure:   cfg.Auth.SecureCookie,
		CookieHTTPOnly: true,
		CookieSameSite: http.SameSiteStrictMode,
		ErrorHandler: func(c *echo.Context, err error) error {
			return echo.NewHTTPError(http.StatusForbidden, "CSRF 校验失败").Wrap(err)
		},
	}))

	// 登录限流按客户端 IP 计算：允许短时间尝试 5 次，之后约每 12 秒恢复一次。
	loginLimiter := middleware.RateLimiterWithConfig(middleware.RateLimiterConfig{
		Store: middleware.NewRateLimiterMemoryStoreWithConfig(
			middleware.RateLimiterMemoryStoreConfig{
				Rate: 5.0 / 60.0, Burst: 5, ExpiresIn: 3 * time.Minute,
			},
		),
		IdentifierExtractor: func(c *echo.Context) (string, error) {
			return c.RealIP(), nil
		},
		ErrorHandler: func(c *echo.Context, err error) error {
			return echo.NewHTTPError(http.StatusForbidden, "无法识别客户端").Wrap(err)
		},
		DenyHandler: func(c *echo.Context, identifier string, err error) error {
			return echo.NewHTTPError(http.StatusTooManyRequests, "登录尝试过于频繁，请稍后再试").Wrap(err)
		},
	})

	authRoutes := e.Group("/api/auth")
	authRoutes.Use(bodyLimit(64*1024, "认证请求内容超过大小限制"))
	authRoutes.POST("/login", auth.Login, loginLimiter)
	authRoutes.POST("/logout", auth.Logout)
	authRoutes.GET("/me", auth.Me, auth.RequireLogin())
	// 老旧浏览器或非浏览器客户端可先读取此 Token，再通过 X-CSRF-Token 请求头提交修改请求。
	authRoutes.GET("/csrf", func(c *echo.Context) error {
		token, err := echo.ContextGet[string](c, "csrf")
		if err != nil {
			return echo.NewHTTPError(http.StatusInternalServerError, "读取 CSRF Token 失败").Wrap(err)
		}
		return c.JSON(http.StatusOK, map[string]string{"token": token})
	})

	// API Token 只能由已登录管理员创建和撤销，明文只在创建响应中出现一次。
	tokenRoutes := e.Group("/api/tokens", auth.RequireLogin())
	tokenRoutes.Use(bodyLimit(64*1024, "Token 请求内容超过大小限制"))
	tokenRoutes.POST("", tokens.Issue)
	tokenRoutes.GET("", tokens.List)
	tokenRoutes.DELETE("/:id", tokens.Revoke)

	// 图片管理接口必须登录；所有修改请求还会经过上面的全局 CSRF 校验。
	imageRoutes := e.Group("/api/images", auth.RequireLogin())
	imageRoutes.GET("", images.List)
	imageRoutes.GET("/trash", images.ListTrash)
	imageRoutes.GET("/:public_id", images.Get)
	imageRoutes.POST("", images.Upload, bodyLimit(cfg.Image.MaxUploadSize+1<<20, "上传内容超过大小限制"))
	imageRoutes.DELETE("/:public_id", images.SoftDelete)
	imageRoutes.POST("/:public_id/restore", images.Restore)
	imageRoutes.DELETE("/:public_id/permanent", images.DeletePermanently)

	// 随机 public_id 相当于不可枚举的公开地址；软删除后 Service 会拒绝继续读取。
	e.GET("/images/*", images.ServeImage)
	e.GET("/thumbnails/*", images.ServeThumbnail)

	// PicGo/Obsidian 使用此接口，不依赖网页登录 Cookie 和 CSRF Token。
	e.POST(
		"/api/upload",
		images.UploadForPicGo,
		tokens.RequireToken(),
		bodyLimit(cfg.Image.MaxUploadSize+1<<20, "上传内容超过大小限制"),
	)

	e.GET("/api/health", func(c *echo.Context) error {
		return c.JSON(http.StatusOK, map[string]string{"status": "ok"})
	})

	return e
}

// bodyLimit 将 Echo 默认的英文 413 错误转换为统一的中文错误信息。
func bodyLimit(limit int64, message string) echo.MiddlewareFunc {
	limiter := middleware.BodyLimit(limit)
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		limited := limiter(next)
		return func(c *echo.Context) error {
			err := limited(c)
			if errors.Is(err, echo.ErrStatusRequestEntityTooLarge) {
				return echo.NewHTTPError(http.StatusRequestEntityTooLarge, message)
			}
			return err
		}
	}
}
