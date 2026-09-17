# Kaven Docker 使用及功能说明

## 1. 产品定位

Kaven Docker 是一个 Windows 桌面端 Docker Registry 管理工具，通过 Docker Registry HTTP API V2 直接访问镜像仓库。它不依赖本机 Docker CLI，可用于浏览镜像、查看 Manifest、删除 Manifest，以及在不同 Registry 之间同步指定镜像 Tag。

首版支持 Windows，界面支持简体中文和英文。应用使用 Vue 3 构建界面，由 Tauri/Rust 完成网络请求、认证、TLS、代理、凭据存储和镜像数据传输。

## 2. 功能概览

| 功能 | 当前能力 |
| --- | --- |
| Registry 管理 | 添加、编辑、删除多个 Registry 连接 |
| 公共 Registry | 内置 Docker Hub、GHCR、Quay.io、Kubernetes Registry、MCR |
| 身份认证 | 匿名访问、用户名和密码（Basic/Bearer Token 挑战） |
| TLS | HTTPS、自定义 CA、跳过证书校验、不安全 HTTP |
| 镜像浏览 | Repository、Tag、Manifest、Digest、平台、Layer、大小、创建时间和 Labels |
| 内容复制 | 复制镜像名、Tag、完整引用、Digest 和 `docker pull` 命令 |
| 镜像删除 | 按 Manifest Digest 删除，需服务端允许删除 |
| 镜像同步 | 选择来源、目标 Repository 和一个或多个 Tag 进行同步 |
| 多架构镜像 | 保留 Manifest Index/List 及其全部平台 Manifest |
| 代理 | 全局默认代理、Registry 独立代理、直连；支持 HTTP、HTTPS 和常用 SOCKS 类型 |
| 诊断 | 分阶段进度、字节进度、详细请求日志和持久化日志文件 |
| 本地化 | 简体中文、英文 |

## 3. 添加 Registry

点击左侧 **Registry** 标题旁的 `+`，打开添加窗口。

### 3.1 使用公共 Registry 预设

可以直接选择以下预设：

- Docker Hub：`https://registry-1.docker.io`
- GitHub Container Registry：`https://ghcr.io`
- Quay.io：`https://quay.io`
- Kubernetes Registry：`https://registry.k8s.io`
- Microsoft Container Registry：`https://mcr.microsoft.com`

预设会以匿名方式添加。需要访问私有镜像时，可在连接设置中改为用户名和密码认证。

多数公共 Registry 不开放全局 Catalog API，因此应用无法通过 Catalog 枚举全部 Repository。添加公共 Registry 后，点击 Repository 列表上方的 `+`，输入用户或命名空间。Kaven Docker 会自动查找 Docker Hub 和 Quay 仓库；GHCR 查找会使用连接中配置的 GitHub Token，并需要 `read:packages` 权限。勾选要添加的项目即可。仍可手动输入完整 Repository 名称作为后备方式，例如：

```text
library/nginx
kavenzero/kaven-media-server
owner/image
dotnet/runtime
```

不要输入协议、Registry Host、Tag 或 Digest。例如应填写 `library/nginx`，而不是 `docker.io/library/nginx:latest`。

如需从 Kaven Docker 中移除已保存的 Repository，请先选中它，再点击 Repository 数量旁的移除按钮。此操作只删除本地保存项，不会删除远程 Repository 或任何镜像。

### 3.2 添加私有 Registry

填写以下内容：

- **显示名称**：仅用于应用内识别，例如 `Production Registry`。
- **Registry URL**：包含协议和可选端口，例如 `https://registry.example.com` 或 `http://192.168.1.20:5000`。
- **认证方式**：匿名，或用户名和密码。
- **自定义 CA 文件**：私有 CA 签发证书时填写本机 PEM 证书路径。
- **允许不安全 HTTP**：URL 使用 `http://` 时必须启用，仅建议用于可信内网。
- **跳过 TLS 证书校验**：仅用于临时诊断，不建议用于生产环境。
- **请求超时**：范围为 3 至 120 秒。
- **代理设置**：跟随全局、直连或使用已保存的代理。

保存后，应用会请求 `/v2/` 测试连接。顶部出现连接异常时，可以打开连接设置检查地址、认证、证书、代理和超时配置。

Registry URL 只填写服务根地址，不要附加 `/v2/`、Repository 或 Tag：

```text
正确：https://registry.example.com
正确：https://registry.example.com:5000
错误：https://registry.example.com/v2/
错误：https://registry.example.com/team/image:latest
```

## 4. 浏览镜像

主界面由三部分组成：左侧选择 Registry，中间选择 Repository 和 Tag，右侧查看镜像详情。

私有 Registry 会通过 Catalog API 分页读取 Repository；公共 Registry 使用手动保存的 Repository 列表。Repository 和 Tag 均可搜索过滤。

镜像详情包括：

- 可直接执行的 `docker pull` 命令
- 镜像名、Tag 和完整镜像引用
- Manifest Digest 与 Media Type
- 镜像总大小和创建时间
- 操作系统、CPU 架构和 Variant
- Config 与文件系统 Layer
- OCI/Docker Image Labels
- 多架构镜像包含的平台和子 Manifest Digest

各引用字段旁的复制按钮可以复制对应内容。

### 4.1 为什么创建时间显示 Unknown

Manifest Index 或 Docker Manifest List 本身通常不包含镜像创建时间。应用会继续读取它引用的平台 Manifest 和 Image Config，并从平台配置中解析创建时间。

以下情况仍可能显示 `Unknown`：

- 上游镜像 Config 没有 `created` 字段。
- 当前账号无权读取对应 Blob。
- Registry 返回了非标准或不完整的 Manifest。
- 网络或代理导致平台 Manifest/Image Config 请求失败。

此时可以刷新重试，并检查连接状态和详细错误信息。`Unknown` 不表示镜像内容损坏。

## 5. 删除镜像 Manifest

选择 Tag 后，在详情区域底部点击 **删除**。应用会先解析该 Tag 对应的 Digest，再向 Registry 发送 Manifest 删除请求。

目标 Registry 必须允许删除。CNCF Distribution 通常需要在服务端配置：

```yaml
storage:
  delete:
    enabled: true
```

注意事项：

- 删除 Manifest 不一定立即释放磁盘空间，通常还需要在 Registry 服务端执行 Garbage Collection。
- 多个 Tag 可能引用相同 Manifest 或 Blob；删除前应确认引用关系。
- 删除操作无法通过 Kaven Docker 撤销。

## 6. 同步镜像

点击顶部 **同步镜像**，可以把来源 Registry 中选定的 Tag 复制到一个私有 Registry。

切换到 **批量同步** 后，可以选择多个来源镜像。每个已选镜像都有独立的目标 Repository 名称和 Tag 选择；如果来源中存在 `latest` 或 `stable`，Kaven Docker 会默认勾选它们，开始前仍可自由修改。来源和目标客户端只初始化一次，各镜像按顺序处理。同步期间会显示总进度、当前镜像以及剩余、成功和失败数量。单个镜像失败不会中断后续任务；全部处理完成后可以查看失败清单，并通过 **重试失败项** 只重新同步失败的镜像。

### 6.1 操作步骤

1. 选择来源 Registry 和 Repository。
2. 选择目标私有 Registry，并填写目标 Repository。
3. 勾选一个或多个需要同步的 Tag。
4. 点击 **开始同步**。
5. 查看各阶段进度和详细日志。

公共 Registry 当前可以作为来源，但不能作为目标。同步到 Docker Hub、GHCR 等公共服务通常需要提供方特定的认证、权限和使用规则，因此首版只允许同步到自建私有 Registry。

### 6.2 同步过程

同步任务包含以下阶段：

1. **初始化源和目标连接**：分别加载来源和目标的认证、TLS、超时及代理设置。
2. **读取并解析 Manifest**：读取所选 Tag，递归解析多架构 Index/List 和平台 Manifest。
3. **检查并传输 Blob**：通过 HEAD 检查目标是否已有 Blob；已有内容直接跳过，缺失内容通过持久化的内容寻址本地缓存分块上传。
4. **写入目标 Manifest**：先写入平台 Manifest，再写入依赖它们的 Index/List。
5. **发布目标 Tag**：将每个所选 Tag 指向目标中的对应 Manifest。

内容以 Digest 去重。因此多个 Tag 或平台共享的 Layer 不会重复传输；目标已存在的 Blob 也会被跳过。下载的 Blob 会保留在应用缓存中，每次复用前验证 SHA-256 或 SHA-512 摘要，从而避免同步到其他目标时重复下载。

同步窗口支持最大化。每个阶段都有独立进度条；Blob 阶段还显示当前字节进度。任务运行时可以点击 **停止同步**；直接关闭同步窗口也会先取消当前网络传输。任务结束后会显示已同步 Tag 数、复制及跳过的 Blob 数、缓存命中数和实际上传字节数。

### 6.3 多架构镜像

对于 OCI Image Index 或 Docker Manifest List，应用会同步顶层 Index/List、全部平台 Manifest、各平台使用的 Config/Layer Blob，以及所选 Tag 与顶层 Manifest 的关联。同步后，Docker 客户端仍可根据本机平台选择正确的镜像。

### 6.4 来源和目标如何使用代理

来源和目标会分别创建独立的 HTTP 客户端，各自使用自己的 Registry 设置：

- 从来源读取 Manifest 和下载 Blob：使用**来源 Registry** 的代理、认证、CA 和超时。
- 检查目标 Blob、上传 Blob、写入 Manifest：使用**目标 Registry** 的代理、认证、CA 和超时。

例如，Docker Hub 可以通过本机 SOCKS5 代理访问，而内网目标 Registry 设置为直连。同步不会把来源代理用于目标连接。

### 6.5 同步后的镜像名称

普通同步是将内容复制到目标 Registry，并不是透明镜像加速。例如：

```text
来源：docker.io/library/nginx:latest
目标：registry.example.com/library/nginx:latest
```

拉取目标镜像时必须使用目标 Host：

```powershell
docker pull registry.example.com/library/nginx:latest
```

镜像 Manifest、Config 和 Layer 内容可以与来源完全相同，但 Registry Host 是客户端定位服务的引用组成部分，不属于 Manifest 内容。

如果希望用户仍执行 `docker pull nginx:latest`，同时由私有服务代替 Docker Hub 提供内容，需要部署 Docker Registry pull-through cache，并在 Docker Engine 的 `registry-mirrors` 中配置它。这是 Registry 服务端和 Docker Engine 的功能，不等同于 Kaven Docker 当前的主动复制。

## 7. 代理配置

点击左下角的网络设置按钮进入代理管理页面。可以预先保存多个代理配置，并在全局或不同 Registry 中复用。界面只要求填写代理类型、Host、端口及可选凭据，不需要手动拼接完整 URL。

| 类型 | 默认端口 | 认证 | DNS 解析 |
| --- | ---: | --- | --- |
| HTTP | 8080 | 支持用户名和密码 | 通常由客户端解析 |
| HTTPS | 443 | 支持用户名和密码 | 通常由客户端解析 |
| SOCKS4 | 1080 | 不支持 | 客户端解析 |
| SOCKS4A | 1080 | 不支持 | 代理端解析 |
| SOCKS5 | 1080 | 支持用户名和密码 | 客户端解析 |
| SOCKS5H | 1080 | 支持用户名和密码 | 代理端解析 |

### 7.1 代理优先级

每个 Registry 可以选择以下模式：

- **跟随全局设置**：使用当前全局默认代理；未设置全局代理时直连。
- **直连**：始终不使用代理，即使配置了全局代理。
- **已保存的代理**：固定使用选中的代理配置。

旧版本中直接保存在 Registry 上的自定义代理仍可兼容读取，但建议迁移为可复用代理配置。正在被全局设置或 Registry 引用的代理不能直接删除。

建议代理服务器负责 DNS 解析时使用 SOCKS5H；目标 Registry 位于局域网时通常选择直连。HTTP Registry 与 HTTP Proxy 是两个独立概念，启用代理不会自动允许不安全的 Registry HTTP。

## 8. TLS、凭据与本地数据

- Registry 和代理密码保存在操作系统凭据管理器中，不写入 JSON 配置文件。
- Registry 地址、用户名、代理地址及非敏感设置保存在应用配置目录。
- 自定义 CA 应使用可信 PEM 文件。
- **跳过 TLS 证书校验** 只应在排查问题时短暂启用。
- **允许不安全 HTTP** 会以明文传输请求和可能的认证信息，只应用于受信任网络。

Windows 默认配置目录为：

```text
%APPDATA%\io.kaven.docker\
```

主要文件包括：

```text
registries.json    Registry 连接设置，不包含密码
proxies.json       代理配置，不包含密码
settings.json      全局设置
```

同步日志使用 Tauri 的应用日志目录。在 Windows 上通常位于：

```text
%LOCALAPPDATA%\io.kaven.docker\logs\sync\
```

同步 Blob 使用应用缓存目录中的内容寻址文件；未完成的下载使用临时 `.part` 文件并在同步结束时清理。

## 9. 日志与故障排查

同步窗口中的 **详细日志** 会记录来源和目标端点及代理模式、Manifest 请求、Blob 检查/下载/分块上传、Manifest 写入、Tag 发布、HTTP 状态、Registry 响应及底层网络错误链。日志不会主动记录密码。

可以点击日志区域右上角的复制按钮复制当前日志。窗口底部显示完整日志文件路径，关闭窗口后仍可继续检查。

### 9.1 常见问题

- **无法连接或超时**：检查根 URL、端口、代理、DNS、VPN、防火墙、CA 和请求超时。
- **401**：检查凭据或访问令牌，以及来源 pull、目标 push 权限。
- **403**：连接可能有效，但账号缺少 catalog、pull、push 或 delete 权限。
- **Catalog 无法读取**：公共 Registry 通常禁用 `_catalog`，请手动添加完整 Repository 名称。
- **删除失败**：启用服务端删除，确认 delete 权限，并允许反向代理转发 `DELETE`。
- **Blob 下载失败**：检查来源 pull 权限、来源代理和应用缓存目录的磁盘空间。
- **Blob 上传失败**：检查目标 push 权限、反向代理请求体限制、缓冲和超时。
- **Manifest 写入失败**：目标可能不接受该 Media Type，或认为依赖 Blob 缺失。

若 Registry 位于 Nginx、Traefik 等反向代理之后，还应检查上传大小限制、请求缓冲、连接超时和 `Location` 响应头改写。

## 10. 界面语言

点击左下角的语言按钮可在中文和英文之间切换。选择结果保存在本机 WebView 的 Local Storage 中，下次启动时继续使用。Registry 中的镜像名、Tag、Label 和服务端错误原文不会被翻译。

## 11. 当前限制

- 首版正式支持 Windows，macOS/Linux 尚未提供构建和验证。
- 不支持定时任务或应用关闭后的后台同步。
- 不支持自动镜像整个 Registry 或自动删除目标端多余 Tag。
- 不支持将公共 Registry 作为同步目标。
- 不提供透明 pull-through cache 或 Docker Engine `registry-mirrors` 配置。
- 不执行 Registry 服务端 Garbage Collection。
- 不管理容器、Docker Daemon、Compose 或 Kubernetes 工作负载。
- 停止同步会中断当前传输，但目标 Registry 中已经成功写入的 Blob、Manifest 或 Tag 不会自动回滚。

## 12. 开发与构建

环境要求：Windows 10/11、Node.js 20 或更高版本、Rust stable MSVC toolchain、Microsoft C++ Build Tools 和 Microsoft Edge WebView2 Runtime。

启动开发环境：

```powershell
npm install
npm run tauri dev
```

VS Code 提供两个启动配置：

- **Tauri: Run Dev (no debugger)**：直接启动，不需要 LLDB 扩展。
- **Tauri: Debug Rust (CodeLLDB)**：调试 Rust，需要安装 CodeLLDB。

预启动任务会复用属于当前工作区的 Vite `1420` 端口。如果该端口被其他程序占用，会显示对应 PID，而不会结束未知进程。

验证和构建：

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

Windows NSIS 安装包位于 `src-tauri/target/release/bundle/nsis/`。
