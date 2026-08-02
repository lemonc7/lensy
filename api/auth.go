package api

import (
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
	sessionName      = "lensy_session"
	sessionUserIDKey = "user_id"
	contextUserKey   = "current_user"
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
	// CookieStore 会对 Session 内容先加密再签名，浏览器只能保存，不能读取或篡改 user_id。
	store.Options = &sessions.Options{
		Path:     "/",
		MaxAge:   maxAge,
		HttpOnly: true,
		Secure:   cfg.SecureCookie,
		SameSite: http.SameSiteStrictMode,
	}
	// 同步设置 securecookie 的最大有效期，过期 Cookie 即使仍被发送也无法通过解码。
	store.MaxAge(maxAge)

	return &Auth{service: authService, store: store}, nil
}

type loginRequest struct {
	Username string `json:"username"`
	Password string `json:"password"`
}

func (r loginRequest) validateLogin() error {
	if strings.TrimSpace(r.Username) == "" || r.Password == "" {
		return errors.New("用户名和密码不能为空")
	}
	if utf8.RuneCountInString(strings.TrimSpace(r.Username)) > 100 || len(r.Password) > 1024 {
		return errors.New("用户名或密码长度超过限制")
	}
	return nil
}

func (r loginRequest) validateSetup() error {
	if err := service.ValidateUsername(r.Username); err != nil {
		return err
	}
	return service.ValidatePassword(r.Password)
}

func (h *Auth) Login(c *echo.Context) error {
	var request loginRequest
	if err := c.Bind(&request); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "请求参数格式错误").Wrap(err)
	}
	if err := request.validateLogin(); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, err.Error())
	}

	user, err := h.service.Login(
		c.Request().Context(),
		request.Username,
		request.Password,
	)
	if err != nil {
		return authHTTPError(err)
	}

	if err := h.createSession(c, user.ID); err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, "创建登录状态失败").Wrap(err)
	}

	return c.JSON(http.StatusOK, user)
}

type setupStatusResponse struct {
	Required bool `json:"required"`
}

func (h *Auth) SetupStatus(c *echo.Context) error {
	required, err := h.service.SetupRequired(c.Request().Context())
	if err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, "查询管理员初始化状态失败").Wrap(err)
	}
	return c.JSON(http.StatusOK, setupStatusResponse{Required: required})
}

func (h *Auth) Setup(c *echo.Context) error {
	var request loginRequest
	if err := c.Bind(&request); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "请求参数格式错误").Wrap(err)
	}
	// API 先完成请求校验，避免无效请求进入密码哈希等昂贵业务操作。
	if err := request.validateSetup(); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, err.Error())
	}

	user, err := h.service.CreateFirstAdmin(
		c.Request().Context(),
		request.Username,
		request.Password,
	)
	if err != nil {
		return authHTTPError(err)
	}
	if err := h.createSession(c, user.ID); err != nil {
		return echo.NewHTTPError(http.StatusInternalServerError, "创建登录状态失败").Wrap(err)
	}
	return c.JSON(http.StatusCreated, user)
}

func (h *Auth) createSession(c *echo.Context, userID int64) error {
	// 每次登录都创建一份全新的 Session，不继承请求中可能存在的旧数据。
	session := sessions.NewSession(h.store, sessionName)
	options := *h.store.Options
	session.Options = &options
	session.Values[sessionUserIDKey] = userID
	if err := h.store.Save(c.Request(), c.Response(), session); err != nil {
		return fmt.Errorf("保存登录 Session: %w", err)
	}
	return nil
}

func (h *Auth) Logout(c *echo.Context) error {
	// 即使请求携带的 Cookie 已损坏，也覆盖为立即过期的 Cookie，保证客户端可以退出。
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

// RequireLogin 从加密 Session 中取得用户 ID，并查询数据库确认用户仍然有效。
func (h *Auth) RequireLogin() echo.MiddlewareFunc {
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c *echo.Context) error {
			session, err := h.store.Get(c.Request(), sessionName)
			if err != nil {
				return echo.NewHTTPError(http.StatusUnauthorized, "登录状态无效").Wrap(err)
			}

			userID, ok := session.Values[sessionUserIDKey].(int64)
			if !ok || userID <= 0 {
				return echo.NewHTTPError(http.StatusUnauthorized, "请先登录")
			}

			user, err := h.service.GetUser(c.Request().Context(), userID)
			if err != nil {
				return authHTTPError(err)
			}
			c.Set(contextUserKey, user)
			return next(c)
		}
	}
}

func CurrentUser(c *echo.Context) (model.User, error) {
	user, ok := c.Get(contextUserKey).(model.User)
	if !ok || user.ID <= 0 {
		return model.User{}, echo.NewHTTPError(http.StatusUnauthorized, "请先登录")
	}
	return user, nil
}

func CurrentUserID(c *echo.Context) (int64, error) {
	user, err := CurrentUser(c)
	if err != nil {
		return 0, err
	}
	return user.ID, nil
}

func authHTTPError(err error) error {
	switch {
	case errors.Is(err, service.ErrInvalidCredentials):
		return echo.NewHTTPError(http.StatusUnauthorized, "用户名或密码错误")
	case errors.Is(err, service.ErrUserDisabled):
		return echo.NewHTTPError(http.StatusForbidden, "用户已被禁用")
	case errors.Is(err, service.ErrUserExists):
		return echo.NewHTTPError(http.StatusConflict, "用户名已存在")
	case errors.Is(err, service.ErrAdminInitialized):
		return echo.NewHTTPError(http.StatusConflict, "管理员初始化已经完成")
	case errors.Is(err, service.ErrInvalidInput):
		return echo.NewHTTPError(http.StatusBadRequest, "输入参数无效").Wrap(err)
	case errors.Is(err, service.ErrNotFound):
		return echo.NewHTTPError(http.StatusUnauthorized, "登录用户不存在")
	default:
		return echo.NewHTTPError(http.StatusInternalServerError, "服务器内部错误").Wrap(err)
	}
}
