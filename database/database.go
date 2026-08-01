package database

import (
	"database/sql"
	"embed"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/golang-migrate/migrate/v4"
	"github.com/golang-migrate/migrate/v4/database/sqlite"
	"github.com/golang-migrate/migrate/v4/source/iofs"
	"github.com/lemonc7/lensy/config"
	_ "modernc.org/sqlite"
)

//go:embed migrations/*.sql
var migrationFiles embed.FS

const databasePath = "data/lensy.db"

func New(cfg config.DatabaseConfig) (*sql.DB, error) {
	if err := cfg.Validate(); err != nil {
		return nil, fmt.Errorf("数据库配置无效: %w", err)
	}

	databaseName, err := filepath.Abs(databasePath)
	if err != nil {
		return nil, fmt.Errorf("解析数据库路径: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(databaseName), 0o755); err != nil {
		return nil, fmt.Errorf("创建数据库目录: %w", err)
	}
	dsn, err := cfg.DSN(databaseName)
	if err != nil {
		return nil, fmt.Errorf("构建 SQLite DSN: %w", err)
	}

	db, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("打开 SQLite: %w", err)
	}
	db.SetMaxOpenConns(cfg.MaxOpenConns)
	db.SetMaxIdleConns(cfg.MaxIdleConns)
	db.SetConnMaxLifetime(cfg.ConnMaxLifetime)
	db.SetConnMaxIdleTime(cfg.ConnMaxIdleTime)

	if err := migrateUp(db, databaseName); err != nil {
		db.Close()
		return nil, err
	}
	return db, nil
}

func migrateUp(db *sql.DB, databaseName string) error {
	source, err := iofs.New(migrationFiles, "migrations")
	if err != nil {
		return fmt.Errorf("打开嵌入式迁移文件: %w", err)
	}
	driver, err := sqlite.WithInstance(db, &sqlite.Config{DatabaseName: databaseName})
	if err != nil {
		return fmt.Errorf("创建 SQLite 迁移驱动: %w", err)
	}
	m, err := migrate.NewWithInstance("iofs", source, "sqlite", driver)
	if err != nil {
		return fmt.Errorf("创建数据库迁移器: %w", err)
	}
	if err := m.Up(); err != nil && !errors.Is(err, migrate.ErrNoChange) {
		return fmt.Errorf("执行数据库迁移: %w", err)
	}
	return nil
}
