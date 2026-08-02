package api

import (
	"errors"
	"net/http"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/labstack/echo/v5"
	"github.com/lemonc7/lensy/model"
	"github.com/lemonc7/lensy/service"
)

const contextAPITokenKey = "current_api_token"

type APITokens struct {
	service *service.APIToken
}

func NewAPITokens(tokenService *service.APIToken) (*APITokens, error) {
	if tokenService == nil {
		return nil, errors.New("API Token 服务不能为空")
	}
	return &APITokens{service: tokenService}, nil
}

type issueAPITokenRequest struct {
	Name string `json:"name"`
	TTL  string `json:"ttl"` // Go duration 格式，例如 720h；留空或 0 表示永不过期
}

func (h *APITokens) Issue(c *echo.Context) error {
	var request issueAPITokenRequest
	if err := c.Bind(&request); err != nil {
		return echo.NewHTTPError(http.StatusBadRequest, "请求参数格式错误").Wrap(err)
	}

	request.Name = strings.TrimSpace(request.Name)
	if request.Name == "" || utf8.RuneCountInString(request.Name) > 100 {
		return echo.NewHTTPError(http.StatusBadRequest, "Token 名称长度必须为 1 到 100 个字符")
	}

	var ttl time.Duration
	if rawTTL := strings.TrimSpace(request.TTL); rawTTL != "" && rawTTL != "0" {
		parsed, err := time.ParseDuration(rawTTL)
		if err != nil || parsed <= 0 {
			return echo.NewHTTPError(http.StatusBadRequest, "ttl 必须是正数时间，例如 720h；留空或 0 表示永不过期")
		}
		ttl = parsed
	}

	issued, err := h.service.Issue(c.Request().Context(), request.Name, ttl)
	if err != nil {
		return apiTokenHTTPError(err)
	}
	return c.JSON(http.StatusCreated, issued)
}

func (h *APITokens) List(c *echo.Context) error {
	tokens, err := h.service.List(c.Request().Context())
	if err != nil {
		return apiTokenHTTPError(err)
	}
	return c.JSON(http.StatusOK, map[string][]model.APIToken{"tokens": tokens})
}

func (h *APITokens) Revoke(c *echo.Context) error {
	id, err := strconv.ParseInt(strings.TrimSpace(c.Param("id")), 10, 64)
	if err != nil || id <= 0 {
		return echo.NewHTTPError(http.StatusBadRequest, "Token ID 必须是正整数")
	}
	if err := h.service.Revoke(c.Request().Context(), id); err != nil {
		return apiTokenHTTPError(err)
	}
	return c.NoContent(http.StatusNoContent)
}

// RequireToken 只接受 Authorization: Bearer <token>，避免 Token 出现在 URL 和访问日志中。
func (h *APITokens) RequireToken() echo.MiddlewareFunc {
	return func(next echo.HandlerFunc) echo.HandlerFunc {
		return func(c *echo.Context) error {
			raw, err := bearerToken(c.Request().Header.Get(echo.HeaderAuthorization))
			if err != nil {
				c.Response().Header().Set(echo.HeaderWWWAuthenticate, "Bearer")
				return err
			}

			token, err := h.service.Authenticate(c.Request().Context(), raw)
			if err != nil {
				c.Response().Header().Set(echo.HeaderWWWAuthenticate, "Bearer")
				return apiTokenHTTPError(err)
			}
			c.Set(contextAPITokenKey, token)
			return next(c)
		}
	}
}

func bearerToken(authorization string) (string, error) {
	parts := strings.Fields(authorization)
	if len(parts) != 2 || !strings.EqualFold(parts[0], "Bearer") || parts[1] == "" {
		return "", echo.NewHTTPError(http.StatusUnauthorized, "请提供有效的 Bearer API Token")
	}
	return parts[1], nil
}

func apiTokenHTTPError(err error) error {
	switch {
	case errors.Is(err, service.ErrInvalidToken):
		return echo.NewHTTPError(http.StatusUnauthorized, "API Token 无效")
	case errors.Is(err, service.ErrExpiredToken):
		return echo.NewHTTPError(http.StatusUnauthorized, "API Token 已过期")
	case errors.Is(err, service.ErrRevokedToken):
		return echo.NewHTTPError(http.StatusUnauthorized, "API Token 已撤销")
	case errors.Is(err, service.ErrNotFound):
		return echo.NewHTTPError(http.StatusNotFound, "API Token 不存在或已经撤销")
	case errors.Is(err, service.ErrInvalidInput):
		return echo.NewHTTPError(http.StatusBadRequest, "API Token 参数无效").Wrap(err)
	default:
		return echo.NewHTTPError(http.StatusInternalServerError, "服务器内部错误").Wrap(err)
	}
}
