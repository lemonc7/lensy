package api

import (
	"crypto/subtle"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"unicode/utf8"

	"github.com/gorilla/sessions"
	"github.com/labstack/echo/v5"
	"github.com/lemonc7/lensy/config"
	"github.com/lemonc7/lensy/model"
	"github.com/lemonc7/lensy/service"
)

const (
	sessionName       = "lensy_session"
	sessionVersionKey = "auth_version"
	contextUserKey    = "current_user"
)

type Auth struct {
	service *service.Auth
	store   *sessions.CookieStore
}

func NewAuth(authService *service.Auth, cfg config.AuthConfig) (*Auth, error) {
	if authService == nil {
		return nil, errors.New("认证服务不能为空")
	}
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("认证配置无效: %w", err)
	}

	maxAge := int(cfg.SessionTTL.Seconds())
	store := sessions.NewCookieStore(
		[]byte(cfg.SessionAuthKey),
		[]byte(cfg.SessionEncryptionKey),
	)
	store.Options = &sessions.Options{
		Path:     "/",
		MaxAge:   maxAge,
		HttpOnly: true,
		Secure:   cfg.SecureCookie,
		SameSite: http.SameSiteStrictMode,
	}
	store.MaxAge(maxAge)

	return &Auth{service: authService, store: store}, nil
}

type loginRequest struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

func (r loginRequest) validate() error {
	if strings.TrimSpace(r.Username) == "" || r.Password == "" {
		return errors.New("用户名和密码不能为空")
	}
	if utf8.RuneCountInString(strings.TrimSpace(r.Username)) > 100 || len(r.Password) > 1024 {
		return errors.New("用户名或密码长度超过限制")
	}
	return nil
}

func (h *Auth) Login(c *echo.Context) error {
	var request loginRequest
	if err := c.Bind(&request); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "请求参数格式错误").Wrap(err)
	}
	if err := request.validate(); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, err.Error())
	}

	user, err := h.service.Login(c.Request().Context(), request.Username, request.Password)
	if err != nil {
		return authHTTPError(err)
	}
	if err := h.createSession(c); err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, "创建登录状态失败").Wrap(err)
	}
	return c.JSON(http.StatusOK, user)
}

func (h *Auth) createSession(c *echo.Context) error {
	session := sessions.NewSession(h.store, sessionName)
	options := *h.store.Options
	session.Options = &options
	session.Values[sessionVersionKey] = h.service.SessionVersion()
	if err := h.store.Save(c.Request(), c.Response(), session); err != nil {
		return fmt.Errorf("保存登录 Session: %w", err)
	}
	return nil
}

func (h *Auth) Logout(c *echo.Context) error {
	session := sessions.NewSession(h.store, sessionName)
	options := *h.store.Options
	options.MaxAge = -1
	session.Options = &options
	if err := h.store.Save(c.Request(), c.Response(), session); err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, "退出登录失败").Wrap(err)
	}
	return c.NoContent(http.StatusNoContent)
}

func (h *Auth) Me(c *echo.Context) error {
	user, err := CurrentUser(c)
	if err != nil {
		return err
	}
	return c.JSON(http.StatusOK, user)
}

// RequireLogin 校验加密 Cookie 中的认证版本；修改密码哈希后旧 Session 会自动失效。
func (h *Auth) RequireLogin() echo.MiddlewareFunc {
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c *echo.Context) error {
			session, err := h.store.Get(c.Request(), sessionName)
			if err != nil {
				return echo.NewHTTPError(http.StatusUnauthorized, "登录状态无效").Wrap(err)
			}

			version, ok := session.Values[sessionVersionKey].(string)
			if !ok || subtle.ConstantTimeCompare(
				[]byte(version),
				[]byte(h.service.SessionVersion()),
			) != 1 {
				return echo.NewHTTPError(http.StatusUnauthorized, "请先登录")
			}

			c.Set(contextUserKey, h.service.User())
			return next(c)
		}
	}
}

func CurrentUser(c *echo.Context) (model.User, error) {
	user, ok := c.Get(contextUserKey).(model.User)
	if !ok || user.Username == "" {
		return model.User{}, echo.NewHTTPError(http.StatusUnauthorized, "请先登录")
	}
	return user, nil
}

func authHTTPError(err error) error {
	switch {
	case errors.Is(err, service.ErrInvalidCredentials):
		return echo.NewHTTPError(http.StatusUnauthorized, "用户名或密码错误")
	case errors.Is(err, service.ErrInvalidInput):
		return echo.NewHTTPError(http.StatusBadRequest, "输入参数无效").Wrap(err)
	default:
		return echo.NewHTTPError(http.StatusInternalServerError, "服务器内部错误").Wrap(err)
	}
}
