# lensy 代码审查报告

审查日期：2026-08-29 · 代码规模：约 4200 行 Rust（26 个源文件）

## 0. 一句话结论

**后端内核（存储 / 数据库 / 图片处理 / 状态机）写得相当扎实，测试覆盖也好；但项目目前"跑不起来"，且前端与业务接口层基本还是空的。** 当务之急不是重构，而是把启动路径打通、把已写好的 Service 能力接到 HTTP 上。

---

## 1. 质量基线（实测结果）

| 检查项 | 结果 |
|---|---|
| `cargo clippy --all-targets --all-features` | 零警告 ✅ |
| `cargo fmt --check` | 干净 ✅ |
| `cargo test --features server` | 38 项全部通过 ✅ |

测试分布在 `db` / `service` / `storage` / `processor` / `auth` / `middleware` 六个模块，覆盖了状态机流转、并发去重、分页稳定性、路径穿越、符号链接目录、损坏图片等场景。这个测试密度在同类个人项目里属于上游水平。

---

## 2. 阻塞级问题（必须先解决，否则跑不起来）

### 2.1 现有数据库迁移校验和不一致 → 启动直接失败 ✅ 已解决

`data/lensy.db` 中 `_sqlx_migrations` 记录的 v1 checksum：

```
库中记录 : c6f12db92d6320bce7745880e17e74cf5613e8a352d60c678b0d95a353318bbc...
当前文件 : ba9f42b4fdcee61854adc05dcbb7a9c53a66687e8d93e996bd021a7cb7d636dff...
是否一致 : 否
```

sqlx 在 `Migrator::run` 中会先做 `validate_applied_migrations`，checksum 不匹配即返回
`MigrateError::VersionMismatch(1)`（见 `sqlx-core-0.9.0/src/migrate/migrator.rs:274`），
于是 `connect()` 失败 → `run()` 提前退出。

**附带证据**：该库中还存在 `api_tokens` 表（含 `token_hash` / `revoked_at` 等 8 列），
而当前代码与迁移文件里都没有它。结合 git 历史 `620995d 补全api_token相关业务` 与
`e205816 调整鉴权逻辑，使用配置文件保存`，可以确认：这个库是 api_token 方案时代的遗留，
迁移文件后来被改写过，但数据库没有重建。

**处理方式**（三选一，按场景）：

- 本地开发、无保留数据价值 → 直接删库：`rm -rf data/`，启动时会自动重建。
- 需要保留数据 → 新建 `migrations/0002_drop_api_tokens.sql` 增量清理，
  并用 `sqlx migrate` 重新对齐校验和（注意：**不要**直接改已应用的 migration 文件）。
- 想一劳永逸 → 把 `data/` 视为一次性产物，加入本地重置脚本。

**已解决**：走第一条路径，旧库已删除并重建（`sqlx migrate run`），迁移校验和
与当前 `migrations/0001_init_schema.sql` 一致，可正常启动。遗留的 `api_tokens`
表随旧库一起消失，代码侧无需改动。

### 2.2 `config/config.toml` 不存在，且仓库里没有任何示例

`src/app/server.rs:42` 硬编码：

```rust
let config = load_config("./config/config.toml").map_err(CapturedError::msg)?;
```

而 `config/` 目录在仓库中不存在，`docker-compose.yml` 却挂载了 `./config:/app/config:ro`
——Docker 会静默创建一个空目录，容器启动时同样报"读取配置文件失败"。

**建议**：提供 `config/config.example.toml`，并在 README 写明复制步骤。按当前
`Config` 结构，示例应包含：

```toml
[server]
port = 8080
public_url = "https://images.example.com"   # 决定 cookie 是否带 Secure
tz = "Asia/Shanghai"
request_timeout = 30
max_http_concurrent = 256

[image]
max_upload_size = 10485760      # 字节
max_pixels = 50000000
quality = 82.0                  # 0-100
thumbnail_quality = 75.0        # 0-100
method = 4                      # 0-6，越大越慢越小
thumbnail_max_edge = 480        # 1-16383
max_concurrent_processing = 4

[auth]
username = "admin"              # 6-20 字符
password = "change-me-please"   # 6-20 字符
token = "至少16位的随机字符串"    # 16-64 字符
```

注意 `auth` 段是 `#[serde(default)]`，缺省时三个字段为空串，但 garde 的
`length(min = 6/16)` 会拦截并让启动失败——这个"失败即安全"的设计是对的，别改。

### 2.3 `assets/tailwind.css` 是 0 字节空文件

`main.rs:8` 引用了它：

```rust
static CSS: Asset = asset!("/assets/tailwind.css");
```

但 `assets/tailwind.css` 大小为 0。真正的 Tailwind 源在**根目录** `tailwind.css`：

```css
@import "tailwindcss";
@source "./src/**/*.rs";
```

说明 Tailwind 的构建产物没有被输出到 `assets/`，CSS 目前完全没有生效。需要配置
Tailwind 的输出目标为 `assets/tailwind.css`（或用 PostCSS 管道生成）。

---

## 3. 功能未完成：后端写好了，但没接出去

这是本次审查最重要的一条。当前 HTTP 层实际只有 6 个端点：

| 方法 | 路径 | 鉴权 | 状态 |
|---|---|---|---|
| POST | `/auth/login` | Public | ✅ |
| GET | `/auth/session` | Admin(cookie) | ✅ |
| POST | `/auth/logout` | Admin(cookie) | ✅ |
| POST | `/api/list_images` | Admin(cookie) | ✅ |
| POST | `/api/list_trashed_images` | Admin(cookie) | ✅ |
| GET | `/i/{file_name}.webp` | Public | ✅ |

而 `Service` 中这些方法**在生产代码里没有任何调用点**（grep 确认，仅测试引用）：

- `upload_image` — 上传（含像素去重、并发冲突处理）
- `soft_delete_image` — 移入回收站
- `restore_image` — 从回收站恢复
- `delete_image` — 彻底删除
- `recover_images` — 恢复中断的上传与删除

具体表现：

**a) 上传接口缺失，但鉴权已经为它留好了位置。**
`middleware.rs:84` 明确写了 `path == "/api/v1/images"` → `RequiredAuth::Upload`
（Bearer token 校验），测试也断言了这一分支。但 `/api/v1/images` 这个路由
在整个代码库里不存在。整套 token 上传鉴权目前是空转的。

**b) 恢复任务没有调度器。** —— 已解决，见 4.2。
`recover_images` 实现了完整的状态机接管逻辑
（`claim_stale_uploads_for_deletion` + `defer_image_deletion` 重试队列），
但原本没有任何定时器/启动钩子调用它：一旦发生上传中断，
`uploading` 记录和残留文件会永久留在库里和磁盘上，为它写的那套
`upload_recovery_lock` 读写锁也就失去意义。现已由后台维护任务定期驱动，
并额外加了 5 分钟宽限期。

**c) 前端是空壳。** `App` 组件只渲染了一个 Stylesheet，没有 Router（尽管
`Cargo.toml` 启用了 `router` feature）、没有页面、没有任何 UI。
`AuthController` / `use_auth` / `AuthStatus` 这套状态管理写得很完整，
但没有任何组件消费它。

**d) 缩略图无法访问。** `ImageFileKind::Thumbnail` 和 `open_trashed_image`
都已实现，但没有任何路由暴露它们。公开路由只有原图。

**e) 遗留物。** `api_tokens` 表（见 2.1）；`Cargo.toml` 的 `router` feature
当前无用；根目录 `tailwind.css` 与 `assets/tailwind.css` 疑似重复。

---

## 4. 安全与健壮性建议

现有的安全基础做得不错（路径穿越防护、符号链接目录拒绝、常量时间比较、
`persist_noclobber` 防覆盖），以下是补强项。

> **状态（2026-08-29 更新）**：4.1 / 4.2 / 4.3 已实施完毕，
> clippy 零警告、38 项测试通过；4.4 / 4.5 保留为可选项；
> 原报告中「显式导入 `cfg_select!`」与「路径改环境变量」两条已撤回，见 4.6。

### 4.1 登录限流 ✅ 已实施

**原问题**：`login_admin` 对失败次数没有任何限制。密码校验是 SHA-256 + 常量时间
比较，速度极快，可在线暴力枚举。

**改动**（`backend/auth.rs`、`app/auth/server_functions.rs`、`app/auth/hooks.rs`）：

- `create_admin_session` 的返回值由 `Option<...>` 改为 `LoginOutcome` 枚举，
  区分「凭据错误」与「已被限流」。
- 按用户名统计失败次数：连续 5 次失败后锁定 5 分钟；距上次失败超过 15 分钟
  则重新计票。
- 被锁定时**先于密码校验返回**，既不消耗随机数，密码正确也无法绕过。
- 登录成功即清空该用户名的失败计数。
- HTTP 层返回 429，消息带上剩余秒数；前端 `login_error_message` 直接透出该提示。
- 跟踪的用户名上限 1000，超出时淘汰最久未失败的条目 —— 用大量不同用户名刷探测
  不会撑爆内存。

> 未采用「按 IP 限流」：单管理员场景下按用户名已足够；而按 IP 需要引入可信的
> 客户端地址来源，反向代理后要处理 `X-Forwarded-For`，反而更易于伪造。
> 若后续直连公网且希望更强防护，更值得做的是把密码换成 argon2 慢哈希。

### 4.2 会话清理与后台维护任务 ✅ 已实施

**原问题**：`AuthService.sessions` 是 `Mutex<HashMap<...>>`，过期清理只在新建会话时
`retain` 一次，`find_admin_session` 只删除被查询到的那一条，长期无人登录时过期
session 会持续堆积；同时 `recover_images` 没有任何调度器驱动（见 3.b）。

**改动**（`backend/auth.rs`、`app/server.rs`、`backend/service.rs`、
`backend/config.rs`）：

- 新增 `AuthService::purge_expired_sessions()`，返回清理条数。
- 新增后台维护任务 `spawn_maintenance`，每 `maintenance_interval` 秒（默认 300，
  可在 `[server]` 配置）执行一次：先清理过期会话，再跑 `recover_images`。
- 用 `interval_at(now + interval, interval)` 启动，避免与启动阶段的首次上传竞争；
  服务器停止接受新请求后 `abort` 该任务。
- `recover_images` 增加 5 分钟宽限期（`UPLOAD_STALE_GRACE_SECONDS`）：
  只接管超过宽限期仍未完成的上传。本实例内的上传已由 `upload_recovery_lock`
  保护，这条主要防止多实例共享存储时误接管他人正在进行的上传。

> 会话仍然只存在于内存：进程重启即全部登出，多实例之间不共享。
> 单容器部署没问题；将来若要水平扩展，需要换成共享存储或改为无状态令牌。

### 4.3 过载快速失败 ✅ 已实施

**原问题**：tower 的 `ConcurrencyLimitLayer` 在并发达到上限时是**排队等待**而非
快速失败，突发流量下请求会无限堆积，配合外层超时又会导致大量 504。

**改动**（`Cargo.toml`、`app/server.rs`）：

- `tower` 增加 `load-shed` 特性，`tokio` 增加 `time` 特性。
- 在 `ConcurrencyLimitLayer` 外层叠加 `LoadShedLayer` + `HandleErrorLayer`：
  并发已满时立即返回 503「服务繁忙，请稍后重试」，而不是让请求排队。
- 日志分级：过载记 `warn`，其他错误记 `error`。

### 4.4 公开图片缺少 ETag / Last-Modified（可选，未实施）

目前只有 `Cache-Control: public, max-age=86400`。图片删除后，中间缓存最长
会继续提供一天。图床场景下 id 不可变，可接受；若要更严谨可加 `immutable`
或改用内容哈希做 ETag。

### 4.5 鉴权白名单是"窄黑名单"模式（可选，未实施）

`required_auth` 把所有非 `/api` 路径都判为 Public。当前前端为空所以无影响，
但将来若新增 `/admin/...` 之类的服务端渲染页面，会默认不需要鉴权。
建议改成显式枚举 + 未知 `/api/*` 默认拒绝（目前对 `/api` 的行为已经是正确的 fail-safe）。

### 4.6 两条已撤回的建议

- **`cfg_select!` 需要显式导入** —— 误判，撤回。`cfg_select!` 是 Rust 1.95.0 起
  进入 `std` prelude 的标准宏（`std::prelude::v1::cfg_select`），
  `storage.rs` 与 `server.rs` 中的用法正确，不需要任何导入。
- **配置与数据路径改环境变量** —— 按项目实际情况撤回。项目通过 Docker 部署，
  路径由镜像固定，保持硬编码。

---

## 5. 工程化与交付

- **缺 Dockerfile。** `docker-compose.yml` 引用 `lemonc7/lensy:latest`，但仓库里
  没有 Dockerfile，无法复现构建。同时建议：固定镜像 tag 而非 `latest`、
  加 `healthcheck`、加日志上限（`logging.driver: json-file` + `max-size`）、
  以非 root 用户运行。
- **缺 README。** 至少应说明：配置格式、如何本地启动、如何构建镜像、
  环境变量含义。
- **缺 CI。** 没有 `.github/workflows` 或其他 CI 配置。
- **编译期依赖本地数据库 ✅ 已缓解**：`query_as!` 宏在编译期需要连接真实数据库
  校验，而 `data/` 已被 gitignore，全新 clone 后 `cargo build` 会失败 ——
  本次审查期间就实际触发过一次。现已执行 `cargo sqlx prepare` 生成 `.sqlx/`
  离线查询数据。**请把 `.sqlx/` 提交进版本库**，并在 CI 中设置环境变量
  `SQLX_OFFLINE=true`。已验证：移走整个 `data/` 后
  `SQLX_OFFLINE=true cargo clippy --all-targets --all-features` 依然通过。
  日后若改动 SQL 或迁移，需重新执行 `cargo sqlx prepare` 并提交新的 `.sqlx/`。
  另外注意：空文件（0 字节）的 `.db` 无法被 sqlx 的 bundled SQLite 打开，
  需要先建出有效库再跑迁移。
- **`.env` 里的 `DATABASE_URL` 没有被运行时代码使用**（连接串是硬编码的），
  它只服务于编译期宏校验。容易误导读者以为改它就能换库，建议加注释说明。
- **`Cargo.toml` 可补充**：`[lints]` 段统一 lint 策略、`description` / `license`
  元数据、`[dev-dependencies]`（测试目前借用了 optional 依赖，
  一旦 `--no-default-features` 编译就会挂）。

---

## 6. 值得肯定的设计

这部分建议**保持现状**，它们解决问题的思路是对的：

1. **图片生命周期状态机**：`uploading → active → trashed → deleting → 物理删除`，
   配合 `updated_at` 做 stale 判定、`defer_image_deletion` 做失败重试队列，
   并用 `upload_recovery_lock`（上传持读锁 / 恢复持写锁）防止正在进行的上传被接管。
   并发安全性靠 DB 状态条件更新保证，而不是靠应用层锁，思路正确。
2. **两层去重**：`content_hash`（最终 webp 字节）与 `pixel_hash`（EXIF 方向
   归一化后的像素）分离，且唯一索引只约束 active 状态的 `pixel_hash`
   （部分唯一索引 `WHERE status = 'active'`），因此回收站里的重复图片不会
   阻塞新上传。这个设计很巧。
3. **存储层的原子性与路径安全**：临时文件先 `sync_all` 再用 `persist_noclobber`
   转正、缩略图失败时回滚原图、`validate_key` 拒绝绝对路径 / `.` / `..` / 盘符前缀、
   `create_storage_directory` 拒绝符号链接目录、`remove_image` 幂等。
   文件持久化后才提交数据库记录的顺序也是对的。
4. **防解压炸弹**：解码前先用 `decoder.dimensions()` 校验尺寸与像素数，
   并设置 `limits.max_alloc`，再真正解码。
5. **鉴权细节**：cookie 用 `__Host-` 前缀 + `HttpOnly` + `SameSite=Strict` +
   https 时 `Secure`；Admin 分支对非安全方法做 Origin 校验；session 只存哈希；
   密码与 token 比较用常量时间 fold-XOR；魔法数判格式后固定 decoder，
   不信任扩展名。
6. **错误映射**：`app/error.rs` 把 `ServiceError` 系统性地映射为 HTTP 语义
   （400/404/409/413/422/503），5xx 才记 error 日志，避免 4xx 噪音淹没日志。

---

## 7. 建议的修复顺序

已完成：1（前半，库已重建）、4、7（登录限流与 LoadShed 两项）。

**接下来建议按顺序推进：**

1. 补 `config/config.example.toml`（格式见 2.2）—— 这是目前唯一还挡着本地启动的东西。
2. 修 Tailwind 输出，让 `assets/tailwind.css` 有内容。
3. 实现 `POST /api/v1/images` 上传接口（鉴权已就绪，直接复用 `upload_image`）。
4. 补上删除 / 恢复 / 彻底删除三个 server function。
5. 开始写前端（登录页 → 图库页 → 回收站页）。
6. 把 `.sqlx/` 提交进版本库，补 Dockerfile、README、CI（CI 里设 `SQLX_OFFLINE=true`）。
7. 可选：4.4 的 ETag、4.5 的鉴权白名单改显式枚举。
