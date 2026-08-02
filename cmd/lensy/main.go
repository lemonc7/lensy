package main

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"os/signal"
	"syscall"
	"time"

	"github.com/lemonc7/lensy/api"
	"github.com/lemonc7/lensy/app"
	"github.com/lemonc7/lensy/config"
	"github.com/lemonc7/lensy/database"
	"github.com/lemonc7/lensy/image"
	"github.com/lemonc7/lensy/repo"
	"github.com/lemonc7/lensy/service"
	"github.com/lemonc7/lensy/storage"
)

func main() {
	handled, err := handlePasswordCommand(os.Args[1:])
	if handled {
		if err != nil {
			slog.Error("生成密码哈希失败", "错误", err)
			os.Exit(1)
		}
		return
	}

	ctx, stop := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer stop()

	if err := run(ctx); err != nil {
		slog.Error("应用退出", "错误", err)
		os.Exit(1)
	}
}

func run(ctx context.Context) error {
	cfg, err := config.Load()
	if err != nil {
		return fmt.Errorf("加载配置: %w", err)
	}
	location, err := cfg.Server.Location()
	if err != nil {
		return fmt.Errorf("设置应用时区: %w", err)
	}
	// 在启动其他组件前设置全局时区，使图片日期目录和应用日志保持一致。
	time.Local = location

	db, err := database.New(cfg.Database)
	if err != nil {
		return fmt.Errorf("初始化数据库: %w", err)
	}
	defer func() {
		if err := db.Close(); err != nil {
			slog.Error("关闭数据库失败", "错误", err)
		}
	}()

	queries := repo.New(db)
	authService, err := service.NewAuth(cfg.Auth.Username, cfg.Auth.PasswordHash)
	if err != nil {
		return fmt.Errorf("初始化认证服务: %w", err)
	}
	authAPI, err := api.NewAuth(authService, cfg.Auth)
	if err != nil {
		return fmt.Errorf("初始化认证接口: %w", err)
	}

	processor, err := image.NewProcessor(cfg.Image)
	if err != nil {
		return fmt.Errorf("初始化图片处理器: %w", err)
	}
	store, err := storage.New()
	if err != nil {
		return fmt.Errorf("初始化图片存储: %w", err)
	}
	imageService := service.NewImage(queries, processor, store)
	imageAPI, err := api.NewImages(imageService, cfg.Server.PublicURL)
	if err != nil {
		return fmt.Errorf("初始化图片接口: %w", err)
	}
	tokenService := service.NewAPIToken(queries)
	tokenAPI, err := api.NewAPITokens(tokenService)
	if err != nil {
		return fmt.Errorf("初始化 API Token 接口: %w", err)
	}
	router := app.NewRouter(authAPI, imageAPI, tokenAPI, cfg)

	server := &http.Server{
		Addr:              cfg.Server.Address(),
		Handler:           router,
		ReadHeaderTimeout: cfg.Server.ReadHeaderTimeout,
		ReadTimeout:       cfg.Server.ReadTimeout,
		WriteTimeout:      cfg.Server.WriteTimeout,
		IdleTimeout:       cfg.Server.IdleTimeout,
		MaxHeaderBytes:    cfg.Server.MaxHeaderBytes,
	}

	serverErrors := make(chan error, 1)
	go func() {
		slog.Info("HTTP 服务已启动", "监听地址", server.Addr)
		serverErrors <- server.ListenAndServe()
	}()

	select {
	case err := <-serverErrors:
		if errors.Is(err, http.ErrServerClosed) {
			return nil
		}
		return fmt.Errorf("运行 HTTP 服务: %w", err)
	case <-ctx.Done():
		slog.Info("收到退出信号，开始优雅关闭")
	}

	shutdownCtx, cancel := context.WithTimeout(context.Background(), cfg.Server.ShutdownTimeout)
	defer cancel()
	if err := server.Shutdown(shutdownCtx); err != nil {
		_ = server.Close()
		return fmt.Errorf("优雅关闭 HTTP 服务: %w", err)
	}

	if err := <-serverErrors; err != nil && !errors.Is(err, http.ErrServerClosed) {
		return fmt.Errorf("关闭 HTTP 服务: %w", err)
	}
	slog.Info("HTTP 服务已关闭")
	return nil
}
