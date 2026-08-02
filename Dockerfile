FROM golang:1.26-alpine AS builder

WORKDIR /src

COPY go.mod go.sum ./
RUN go mod download

COPY . .

# 禁用CGO
RUN CGO_ENABLED=0 GOOS=linux go build \
    -trimpath \
    -ldflags="-s -w" \
    -o /out/lensy \
    ./cmd/lensy

# scratch 中没有目录管理工具，因此在构建阶段准备运行目录。
RUN mkdir -p /rootfs/app/config /rootfs/app/data /rootfs/tmp \
    && chmod 1777 /rootfs/tmp


FROM scratch

COPY --from=builder /rootfs/ /
COPY --from=builder /out/lensy /app/lensy

WORKDIR /app

EXPOSE 8080

ENTRYPOINT ["/app/lensy"]
