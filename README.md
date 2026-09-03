# NoteDock

Windows 悬浮记事本 + NAS 服务端 + 浏览器管理端。桌面端是主要入口，笔记集中存在 NAS 上，
浏览器可以远程编辑，断网时桌面端照常编辑、联网后自动同步。

上一代 [PiP Notepad](../README.md) 只能存一篇笔记、数据只在本机、图片以 base64 内联。
NoteDock 是它的继任者。

## 结构

```text
crates/notedock-api       HTTP 契约的唯一真相源；cargo test 会生成 TypeScript 镜像
crates/notedock-server    NAS 服务端（Axum + SQLite）
crates/notedock-desktop   桌面端的 Rust 侧（Tauri 2）：本地库、同步、凭据
packages/editor           两端共用的 Svelte 5 + TipTap 编辑器与设计令牌
apps/web                  浏览器管理端
apps/desktop              悬浮窗界面
deploy/                   Dockerfile 与 compose
```

三个约定支撑起这套结构：

- **契约由 Rust 定义。** `notedock-api` 里的类型经 `ts-rs` 导出到
  `packages/editor/src/generated/`，服务端、桌面端和两个前端读同一份定义。
- **编辑器只写一次。** `packages/editor` 以源码形式被两个应用的 Vite 直接编译，
  没有中间构建产物，也就没有两端行为漂移的空间。
- **桌面端不在 webview 里联网。** 所有 HTTP 都走 Rust，令牌不进 JavaScript，
  Tauri 的 CSP 保持关闭外部来源。

## 同步模型

服务端给每篇笔记一个单调递增的 `rev`，并把每次写入追加到全局变更日志 `note_changes`，
客户端记住看到过的最大 `seq` 作为游标增量拉取。写入时带上 `base_rev`：不匹配就返回 409，
客户端保留服务端版本、把本地改动另存为「冲突副本」——不合并、不丢弃，由人决定。

桌面端本地库里 `dirty = 1` 就是发件箱，`rev = 0` 表示这篇笔记还没到过服务端（离线新建），
需要 POST 而不是 PUT。离线新建的笔记自带客户端分配的 UUID，所以断线重传不会产生重复。

## 开发

本机 `cargo` 和 `pnpm` 都不在 PATH，需要全路径（构建脚本会调用 `rustc`，所以要把
toolchain 的 `bin` 加进 PATH，而不只是用 cargo 的全路径）：

```bash
export PATH="$HOME/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin:$PATH"
export PATH="$HOME/.workbuddy/pnpm-bin:$PATH"
export NODE_OPTIONS="--use-system-ca"   # 绕开注入的 safe-delete 钩子
```

```bash
pnpm install

# 服务端（NOTEDOCK_PASSWORD 只用于本机开发；部署用 NOTEDOCK_PASSWORD_HASH）
NOTEDOCK_PASSWORD='dev-password-123' NOTEDOCK_BIND='127.0.0.1:8080' \
  NOTEDOCK_DB='data/dev.db' cargo run -p notedock-server

pnpm --filter @notedock/web dev        # 浏览器端 :5173，API 由 Vite 代理到 :8080
pnpm --filter @notedock/desktop dev    # 悬浮窗界面 :5174
cargo run -p notedock-desktop          # 悬浮窗本体（读上面的 :5174）

cargo test --workspace                 # 含契约生成与服务端集成测试
pnpm -r check                          # svelte-check
```

改动 `crates/notedock-api` 后跑 `cargo test -p notedock-api` 重新生成 TypeScript。

## 快捷键

| 快捷键 | 作用 |
|---|---|
| Ctrl+P | 搜索并切换笔记 |
| Ctrl+N | 新建笔记（桌面端） |
| Ctrl+S | 立即保存（平时自动保存） |
| Ctrl+, | 打开设置（桌面端） |
| Ctrl+Shift+K | 开关点击穿透（桌面端） |

格式化没有常驻工具栏：选中文字才浮现气泡条。

悬浮窗的标题栏只有三样东西：笔记标题、一个设置按钮、一个同步状态圆点。不透明度、
窗口置顶、点击穿透、桌面置顶、服务器信息、版本与数据目录都在设置面板里。
不透明度和窗口置顶会写进 `settings.json`，下次启动保持原样；点击穿透故意不持久化——
一个启动就穿透的窗口是个陷阱。

## 部署

见 [deploy/docker-compose.yml](deploy/docker-compose.yml)。先生成密码哈希，再启动：

```bash
docker compose -f deploy/docker-compose.yml pull
docker compose -f deploy/docker-compose.yml run --rm --entrypoint notedock-server notedock hash-password '你的密码'
# 把输出写进 deploy/.env 的 NOTEDOCK_PASSWORD_HASH
docker compose -f deploy/docker-compose.yml up -d
```

只发布应用端口，数据库和文件系统不暴露。

推送到 GitHub `main` 后，`.github/workflows/docker.yml` 会自动构建完整的
浏览器端和服务端镜像，并发布到 `ghcr.io/qwejun/notedock:latest`。NAS 上可以直接使用：

```bash
docker pull ghcr.io/qwejun/notedock:latest
```

## 第一阶段的两个已知限制

**纯 HTTP。** 登录令牌和笔记正文明文过公网，同链路可嗅探。密码本身不会被存储或传输明文
比对（服务端只有 Argon2 哈希），但传输层没有加密。

浏览器端的截图粘贴仍然可用——它走 `paste` 事件而不是 `navigator.clipboard.read()`，
后者需要安全上下文。反过来，「复制为富文本」在纯 HTTP 下不可用。
compose 里放开注释里的 Caddy 服务即可开启 HTTPS，应用代码不需要改。

**IPv6-only 可达性。** 只有公网 IPv6 域名时，IPv4-only 的网络打不开浏览器端。
这不是配置问题；需要 Cloudflare Tunnel 之类的 IPv4 入口。

## 还没做

图片与附件上传（契约已留好 `/blobs` 位置）、WebSocket 实时推送、全文搜索、
V1 数据导入、标签与文件夹。
