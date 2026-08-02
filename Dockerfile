FROM golang:1.26-alpine AS builder

WORKDIR /src

COPY go.mod go.sum ./
RUN go mod download

COPY . .

# 项目使用纯 Go SQLite 和 WebP 实现，可以生成不依赖 libc 的静态二进制。
RUN CGO_ENABLED=0 GOOS=linux go build \
    -trimpath \
    -ldflags="-s -w" \
    -o /out/lensy \
    ./cmd/lensy

# scratch 中没有用户和目录管理工具，因此在构建阶段准备运行目录。
RUN mkdir -p /rootfs/app/config /rootfs/app/data /rootfs/tmp \
    && chown -R 65532:65532 /rootfs/app \
    && chmod 1777 /rootfs/tmp


FROM scratch

# 为将来访问外部 HTTPS 服务保留系统根证书。
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder --chown=65532:65532 /rootfs/ /
COPY --from=builder --chown=65532:65532 /out/lensy /app/lensy

WORKDIR /app

# scratch 没有 /etc/passwd，直接使用非 root 数字 UID/GID。
USER 65532:65532

EXPOSE 8080

ENTRYPOINT ["/app/lensy"]
