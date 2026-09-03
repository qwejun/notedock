# NoteDock

NoteDock 是一个放在 Windows 桌面上的悬浮记事本。

笔记保存在 NAS 服务端，电脑上的桌面窗口和浏览器都可以打开、编辑。电脑暂时断网时也能继续写，网络恢复后会自动同步。

## 现在能做什么

- Windows 桌面悬浮窗口，随时打开记录内容
- 笔记有独立的标题和正文，可以新建、切换、搜索
- 浏览器访问 Web 端，远程查看和编辑笔记
- 桌面端和 Web 端使用同一份数据
- 断网时可以继续编辑，联网后自动同步
- 支持窗口置顶、透明度、点击穿透和桌面置顶某一篇笔记
- 支持基础文字格式：标题、粗体、斜体、删除线、列表、引用、文字颜色和高亮
- 登录信息保存在本机，不需要每次启动都重新登录

## 项目目录

```text
apps/desktop                 Windows 桌面端界面
apps/web                     浏览器 Web 端
packages/editor              桌面端和 Web 端共用的编辑器
crates/notedock-server       NAS 上运行的服务端
crates/notedock-desktop      Windows 桌面端的 Rust 部分
crates/notedock-api          前后端共用的数据定义
deploy                       Docker 镜像和 compose 配置
```

## 在电脑上开发

需要先安装 Node.js、pnpm、Rust 和 Docker（Docker 只在本地跑服务端时需要）。

```bash
pnpm install
```

启动本地服务端：

```bash
NOTEDOCK_PASSWORD=dev-password-123 \
NOTEDOCK_BIND=127.0.0.1:8080 \
NOTEDOCK_DB=data/dev.db \
cargo run -p notedock-server
```

启动 Web 端：

```bash
pnpm --filter @notedock/web dev
```

启动桌面端界面：

```bash
pnpm --filter @notedock/desktop dev
```

常用检查：

```bash
cargo test --workspace
pnpm -r check
```

## 部署到 NAS

NAS 上只需要 Docker。镜像会由 GitHub Actions 自动构建并发布，不需要在 NAS 上安装 Rust 或 Node.js。

先下载项目：

```bash
git clone https://github.com/qwejun/notedock.git
cd notedock
```

生成登录密码的哈希：

```bash
docker compose -f deploy/docker-compose.yml run --rm \
  --entrypoint notedock-server notedock \
  hash-password '换成你自己的密码'
```

在 `deploy/.env` 中写入生成的结果：

```env
NOTEDOCK_PASSWORD_HASH=这里填上面生成的整串内容
```

启动服务：

```bash
docker compose --env-file deploy/.env -f deploy/docker-compose.yml pull
docker compose --env-file deploy/.env -f deploy/docker-compose.yml up -d
```

默认端口是 `8080`，浏览器打开：

```text
http://你的IPv6域名:8080
```

也可以在 `deploy/.env` 里修改端口：

```env
NOTEDOCK_PORT=18080
```

数据保存在 Docker volume `notedock-data` 中。升级时执行：

```bash
docker compose --env-file deploy/.env -f deploy/docker-compose.yml pull
docker compose --env-file deploy/.env -f deploy/docker-compose.yml up -d
```

## 自动构建 Docker

每次向 GitHub 的 `main` 分支推送代码，都会自动执行：

1. 构建 Web 前端
2. 编译服务端
3. 构建 Docker 镜像
4. 发布到 GitHub Container Registry

镜像地址：

```text
ghcr.io/qwejun/notedock:latest
```

工作流文件在 `.github/workflows/docker.yml`，运行记录可以在 GitHub 仓库的 Actions 页面查看。

## 安全提醒

当前部署按需求使用 HTTP，没有 HTTPS。公网传输的登录令牌和笔记内容没有加密，只建议在开发或可信网络中使用。正式对公网开放时，建议在前面加 Caddy 或 Nginx，再启用 HTTPS。

如果只有公网 IPv6，只有 IPv4 网络的电脑可能无法访问，需要额外提供 IPv4 入口或使用 Cloudflare Tunnel。

## 目前还没完成

图片和附件上传、全文搜索、标签和文件夹、笔记导入等功能还在后续开发中。
