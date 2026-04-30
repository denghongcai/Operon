# Computer Mesh 项目抽象

Source: https://chatgpt.com/share/69f2f057-2fc8-83a1-87d6-7d804215cbb7

Created: 2026-04-30T06:01:59.198Z

Updated: 2026-04-30T06:02:04.744Z

This document archives the discussion that shaped Operon, including the README draft, architecture notes, implementation roadmap, naming, AI-native positioning, and technology stack suggestions.

---

## 1. User

Time: 2026-04-27T09:42:23.168Z

我想做一个这样的开源项目：实现一个computer的mesh网络。支持将本地、云端的文件系统、图像（可选）、声音（可选）、进程打通，让本地可以操作云端、云端可以操作本地。你看看我这几项抽象足够么。

---

## 2. Assistant

Time: 2026-04-27T09:42:24.918Z

我先按“项目抽象/边界设计”来评估，不直接跳到实现。重点看你的几类资源是否能覆盖「跨设备可操作」这个目标，以及还缺哪些底层抽象。

---

## 3. Assistant

Time: 2026-04-27T09:42:24.918Z

这几项**方向是对的，但抽象还不够**。你现在列的是“资源类型”，还缺一层更核心的抽象：**能力、身份、连接、权限、执行语义**。

我建议把项目抽象成：

```text
Computer Mesh
= 多台计算机之间的 Capability Network
```

也就是不是简单“打通文件/图像/声音/进程”，而是让每台机器暴露一组可授权、可调用、可审计的能力。

比较完整的抽象可以是：

```text
Node        一台设备/虚拟机/容器/云主机
Identity    节点身份、用户身份、设备身份
Transport   P2P / relay / LAN / tunnel
Capability  节点暴露的能力
Resource    文件、屏幕、音频、进程、端口、剪贴板等
Session     一次跨设备操作会话
Policy      谁可以访问什么、以什么权限访问
Audit       操作日志、追踪、回放
```

你列的资源可以归类为：

```text
FileSystemCapability
  - read
  - write
  - watch
  - mount
  - sync
  - search

ProcessCapability
  - list
  - start
  - stop
  - exec
  - attach
  - stream stdout/stderr
  - inspect env/cwd/user

ScreenCapability / ImageCapability
  - screenshot
  - stream screen
  - click/key/input
  - OCR/vision optional

AudioCapability
  - capture
  - play
  - stream
  - transcribe optional
```

但我觉得还应该补这些：

```text
NetworkCapability
  - expose local port
  - forward remote port
  - service discovery
  - reverse tunnel

ClipboardCapability
  - read/write clipboard
  - sync clipboard

SecretCapability
  - access token
  - env secret
  - credentials broker
  - 不直接暴露明文

DeviceInfoCapability
  - OS / arch / hostname
  - GPU / CPU / memory
  - installed tools
  - runtime status

JobCapability
  - long-running task
  - status
  - cancel
  - logs
  - artifacts
```

更关键的是，你需要区分两种模式：

```text
1. Remote Control
本地像控制远程机器一样操作云端/本地。

2. Resource Federation
多台机器的资源被抽象成一个统一命名空间。
```

例如：

```text
mesh://local/fs/Users/me/project
mesh://cloud-a/fs/home/ubuntu/app
mesh://lab-gpu/process/1234
mesh://mac/screen/main
mesh://cloud-a/port/3000
```

我会建议你最终的核心抽象不是：

```text
文件系统 + 图像 + 声音 + 进程
```

而是：

```text
Node + Capability + Resource URI + Session + Policy
```

一个更清晰的项目定位可以写成：

```text
An open-source computer mesh runtime that connects local machines,
cloud machines, containers, and sandboxes into a secure capability network.

Each node can expose selected capabilities such as filesystem, process,
network ports, screen, audio, clipboard, and jobs. Other authorized nodes
can discover, invoke, mount, stream, or automate these capabilities through
a unified URI and permission model.
```

所以结论是：

**你这几项是很好的第一批 Capability，但还不是完整抽象。**

我建议最小 MVP 先做：

```text
1. Node identity
2. Secure transport
3. Filesystem capability
4. Process/job capability
5. Port forwarding capability
6. Permission policy
7. CLI
```

图像、声音、桌面控制可以放第二阶段。  
否则一开始范围会像 AnyDesk + SSH + Tailscale + FUSE + job runner + cloud IDE，太大。

---

## 4. User

Time: 2026-04-27T09:43:56.203Z

SecretCapability
  - access token
  - env secret
  - credentials broker
  - 不直接暴露铭文

这是用来干啥的，感觉没必要？

---

## 5. Assistant

Time: 2026-04-27T09:43:57.055Z

你这个直觉**短期是对的，但长期一定会踩坑**。  
我直接说结论：**SecretCapability 不是“锦上添花”，而是你这个系统能不能真正可用的分水岭。**

---

## 先说本质问题（你现在的模型缺的东西）

你现在的模型是：

```text
A 节点 可以操作 B 节点
```

但真实世界是：

```text
A 想让 B 去操作 C（需要凭证）
```

举几个你这个系统**必然会遇到的场景**👇

---

## 场景1：远程执行任务（最典型）

```text
你在本地
→ 调用云端节点
→ 云端去 pull 私有 GitHub repo
```

问题来了：

👉 GitHub token 在哪？

### 你现在的模型会变成：

```text
把 token 明文传过去
```

然后你系统就变成：

```text
日志里有 token
内存里有 token
agent 可以偷 token
其他节点可以复用 token
```

基本等于：

> 🔥 “一旦某个节点被入侵，全网沦陷”

---

## 场景2：你做 agent / 自动化（你肯定会做）

比如：

```text
mesh://cloud/run "deploy service"
```

这个 deploy 里面会用：

- AWS key
- Docker registry token
- SSH key

你有两个选择：

### ❌ 方案A：用户自己管

```text
用户自己在每台机器配置一遍 secret
```

结果：

- 多节点同步灾难
- token 泄漏不可控
- rotation 做不了
- 用户体验差

---

### ❌ 方案B：你帮他传

```text
mesh 自动把 secret 带过去
```

结果：

- 明文传输（哪怕 TLS，也有落盘/日志问题）
- 节点之间可以窃取
- 无法审计

---

## SecretCapability 在解决什么

它的目标其实很简单：

```text
让“能力”使用 secret，而不是“用户代码”直接接触 secret
```

---

## 正确模型（很关键）

不是这样：

```text
run("git clone", token="xxx")
```

而是这样：

```text
run("git clone", useCredential="github:repo-read")
```

然后：

```text
SecretCapability 负责：
- 在本机找 credential
- 不暴露给调用方
- 只在执行上下文中注入
- 可审计
- 可撤销
```

---

## 更直白一点

你可以理解成：

```text
SecretCapability = Mesh 里的 “AWS IAM”
```

或者：

```text
SecretCapability = “不让 token 变成字符串”的机制
```

---

## 如果你不做，会发生什么

你这个系统最终会变成：

```text
一个超级危险的 SSH + rsync + screen + exec 集合体
```

然后用户会：

- 不敢放生产环境
- 不敢接云资源
- 不敢做自动化

最终只能当：

> 🪦 “玩具项目 / demo”

---

## 最小可行版本（你可以这样收敛）

我不建议你一开始做完整 vault，那太重了。

你可以先做一个**极简版 SecretCapability**：

```text
SecretCapability (MVP)

- 本地存储（文件 / keychain）
- 按 name 管理：
    github:repo-read
    aws:prod
- 只能在 node 内使用，不可读出
- 执行时注入 env（但不回传）
- CLI 管理：
    mesh secret add
    mesh secret list
    mesh secret revoke
```

执行时：

```bash
mesh run cloud-a \
  --use-secret github:repo-read \
  "git clone ..."
```

---

## 再往上一层（未来你一定会做）

```text
- Secret scope（只允许某节点使用）
- Secret TTL
- Secret delegation（A 允许 B 用，但不能转给 C）
- 审计日志（谁用了什么 secret）
- 硬件隔离（TPM / enclave 可选）
```

---

## 总结一句话

👉 你现在在设计的是：

```text
“机器之间可以互相操作”
```

但真正要做的是：

```text
“机器之间可以安全地代理彼此的能力”
```

而：

> **SecretCapability = 安全代理的核心基础设施**

---

如果你愿意，我可以帮你把整个项目抽象成一个**类似 “mesh:// 协议 + capability 调用模型 + 安全边界” 的完整架构图**，那会更接近一个真正能做大的开源项目。

---

## 6. User

Time: 2026-04-27T09:45:23.793Z

有道理。那你先给我画个模块架构图吧，让我先有个全局概念。

---

## 7. User

Time: 2026-04-27T09:47:52.108Z

在读写文件上，我希望在各个节点上都能表现为一个和本地文件非常像的操作形式（比如就是读写文件），你觉得应该怎么做

---

## 8. Assistant

Time: 2026-04-27T09:47:58.281Z

我建议做成 **“虚拟文件系统 + 协议代理”**，核心目标是：

```text
让远端资源在本机看起来像一个普通目录
```

例如：

```bash
ls ~/mesh/cloud-a/home/ubuntu/project
cat ~/mesh/mac/Users/me/a.txt
echo hello > ~/mesh/gpu-node/tmp/test.txt
```

背后实际访问的是：

```text
mesh://cloud-a/fs/home/ubuntu/project
mesh://mac/fs/Users/me/a.txt
mesh://gpu-node/fs/tmp/test.txt
```

## 推荐架构

```text
应用 / shell / IDE / agent
        ↓
本地文件路径
        ↓
Virtual FS Layer
        ↓
Mesh FS Client
        ↓
Mesh Transport
        ↓
远端 Node Agent
        ↓
真实文件系统
```

## 最像本地文件的方案

### Linux / macOS

用 **FUSE**。

```text
~/mesh/
  cloud-a/
    home/ubuntu/...
  macbook/
    Users/me/...
  nas/
    photos/...
```

优点是体验最好：

```bash
cd ~/mesh/cloud-a/home/ubuntu/app
vim package.json
grep -R "foo" .
code .
```

这些工具都能直接用。

### Windows

可以用：

```text
WinFsp
```

效果类似 FUSE，可以挂载成：

```text
M:\cloud-a\home\ubuntu\app
```

或者：

```text
\\mesh\cloud-a\home\ubuntu\app
```

## 但不要一开始就做完整 POSIX

这是大坑。

你应该把能力分层：

```text
Level 1: File API
- readFile
- writeFile
- stat
- listDir
- mkdir
- remove
- rename

Level 2: Streaming
- open
- read range
- write range
- close
- upload/download large file

Level 3: Watch / Sync
- watch directory
- incremental sync
- conflict handling

Level 4: Mount
- FUSE / WinFsp / WebDAV / NFS-like gateway
```

MVP 我建议先做：

```text
Mesh FS API + CLI
```

例如：

```bash
mesh fs ls cloud-a:/home/ubuntu
mesh fs cat cloud-a:/home/ubuntu/a.txt
mesh fs cp ./local.txt cloud-a:/tmp/local.txt
mesh fs cp cloud-a:/tmp/a.log ./a.log
```

然后再做挂载：

```bash
mesh mount cloud-a:/home/ubuntu ~/mesh/cloud-a
```

## 关键设计点

你不要让 FUSE 直接等于核心协议。  
正确结构应该是：

```text
FUSE 只是一个 adapter
```

核心还是：

```text
Mesh FS Protocol
```

也就是：

```text
FUSE Adapter
CLI Adapter
SDK Adapter
Agent Adapter
        ↓
Mesh FS Protocol
        ↓
Remote Node FS Capability
```

这样未来可以支持：

```text
CLI
IDE 插件
Web 控制台
Agent 工具调用
FUSE 挂载
WebDAV 网关
```

## 文件路径建议

内部统一用 URI：

```text
mesh://cloud-a/fs/home/ubuntu/project/package.json
```

用户侧可以映射成本地路径：

```text
~/mesh/cloud-a/home/ubuntu/project/package.json
```

## 权限模型很重要

每个节点不应该默认暴露整个磁盘。

应该这样：

```yaml
capabilities:
  fs:
    mounts:
      - name: workspace
        path: /home/ubuntu/workspace
        permissions:
          read: true
          write: true
          delete: false
      - name: logs
        path: /var/log/myapp
        permissions:
          read: true
          write: false
```

外部访问：

```text
mesh://cloud-a/fs/workspace/project
mesh://cloud-a/fs/logs/app.log
```

不要一上来暴露：

```text
mesh://cloud-a/fs/
```

这个太危险。

## 最终建议

你的文件系统抽象可以定成：

```text
FilesystemCapability
  - 统一 URI
  - 节点侧受限目录暴露
  - 核心 FS 协议
  - 多 adapter：CLI / SDK / FUSE / WinFsp / WebDAV
  - 支持 stat/read/write/list/watch
  - 后续支持 cache/sync/conflict
```

最推荐的落地路线：

```text
第一阶段：mesh fs CLI
第二阶段：Node Agent 暴露受限目录
第三阶段：大文件 streaming + range read/write
第四阶段：FUSE/WinFsp 挂载
第五阶段：watch/sync/cache/conflict
```

一句话：  
**核心不要做“远程文件复制工具”，而要做“可挂载的 Mesh Filesystem Capability”。**

---

## 9. User

Time: 2026-04-27T09:49:59.946Z

ok

---

## 10. Assistant

Time: 2026-04-27T09:50:01.756Z

👍 这个方向是对的，而且你如果把 **FS capability 做对**，整个项目就已经有 60% 价值了。

我给你一个**可以直接开干的最小设计稿（偏终态，但可分阶段实现）**，尽量贴近你现在的工程习惯（TS / agent / tool schema 那套）。

---

# 一、核心协议（你真正要先定死的）

你不要先写 FUSE，先把这个协议定好：

```ts
// mesh fs protocol (MVP)

type NodeId = string
type Path = string

interface Stat {
  isFile: boolean
  isDir: boolean
  size: number
  mtime: number
}

interface MeshFS {
  stat(node: NodeId, path: Path): Promise<Stat>

  readFile(node: NodeId, path: Path): Promise<Uint8Array>

  writeFile(
    node: NodeId,
    path: Path,
    content: Uint8Array,
    opts?: { create?: boolean; truncate?: boolean }
  ): Promise<void>

  listDir(node: NodeId, path: Path): Promise<string[]>

  mkdir(node: NodeId, path: Path): Promise<void>

  remove(node: NodeId, path: Path): Promise<void>

  rename(node: NodeId, from: Path, to: Path): Promise<void>
}
```

👉 这是你**整个系统 FS 的“内核 API”**  
👉 后面所有东西（CLI / FUSE / agent）都走它

---

# 二、URI 设计（非常关键）

统一资源表达：

```text
mesh://<node-id>/fs/<mount>/<path>
```

例子：

```text
mesh://cloud-a/fs/workspace/project/package.json
mesh://mac/fs/home/Downloads/a.png
```

解析后变成：

```ts
{
  node: "cloud-a",
  capability: "fs",
  mount: "workspace

---

## 11. User

Time: 2026-04-27T09:50:19.032Z

那给个完整的整体实现路线图

---

## 12. Assistant

Time: 2026-04-27T09:50:21.094Z

我会按“开源项目可落地”的路线来拆：先定内核协议和最小闭环，再扩展到挂载、进程、权限、安全和生态，不会一上来铺太大。

---

## 13. Assistant

Time: 2026-04-27T09:50:21.094Z

下面是一个比较完整、但仍然可执行的路线图。

# Computer Mesh 实现路线图

## 0. 项目定位

一句话：

```text
Computer Mesh 是一个把多台本地/云端/容器机器连接成统一能力网络的开源运行时。
```

核心不是“远程桌面”或“文件同步”，而是：

```text
Node + Capability + Resource URI + Secure Session + Policy
```

---

# 1. Phase 1：最小可用内核

目标：先让两台机器能安全连接，并完成文件读写。

## 1.1 Node Agent

每台机器运行一个 agent：

```text
mesh-agent
```

负责：

```text
- 注册节点身份
- 暴露本机 capability
- 接收远程调用
- 执行本地操作
- 返回结果 / stream
```

节点示例：

```text
local-mac
cloud-a
gpu-node
home-nas
```

## 1.2 Node Identity

先用简单设备身份：

```text
NodeId
PublicKey
PrivateKey
NodeName
DeviceInfo
```

最小实现：

```text
~/.mesh/node.json
~/.mesh/node.key
```

以后再接入：

```text
OIDC / SSO / WebAuthn / TPM
```

## 1.3 Mesh Transport

MVP 先支持：

```text
direct TCP / WebSocket
```

后续再加：

```text
QUIC
Relay
NAT traversal
LAN discovery
WebRTC
```

最小协议：

```text
request / response
stream
cancel
heartbeat
```

## 1.4 Filesystem Capability

第一阶段只做：

```text
stat
listDir
readFile
writeFile
mkdir
remove
rename
```

不要一上来做完整 POSIX。

统一 URI：

```text
mesh://cloud-a/fs/workspace/project/package.json
```

节点配置：

```yaml
capabilities:
  fs:
    mounts:
      - name: workspace
        path: /home/ubuntu/workspace
        permissions:
          read: true
          write: true
          delete: false
```

CLI：

```bash
mesh fs ls cloud-a:/workspace
mesh fs cat cloud-a:/workspace/a.txt
mesh fs cp ./a.txt cloud-a:/workspace/a.txt
mesh fs cp cloud-a:/workspace/log.txt ./log.txt
```

---

# 2. Phase 2：CLI 和 SDK 闭环

目标：让开发者能真正开始使用。

## 2.1 CLI

命令形态：

```bash
mesh node list
mesh node info cloud-a

mesh fs ls cloud-a:/workspace
mesh fs cat cloud-a:/workspace/package.json
mesh fs cp ./local.txt cloud-a:/workspace/local.txt

mesh run cloud-a "pwd && ls"
mesh job list
mesh job logs <job-id>
mesh job cancel <job-id>
```

## 2.2 TypeScript SDK

你可以先做 TS SDK，因为你的生态更适合 agent / web / IDE。

```ts
const mesh = new MeshClient()

const content = await mesh.fs.readFile(
  "mesh://cloud-a/fs/workspace/package.json"
)

await mesh.fs.writeFile(
  "mesh://cloud-a/fs/workspace/a.txt",
  Buffer.from("hello")
)
```

## 2.3 Resource URI Parser

这个要早做，而且要稳定。

```ts
parseMeshUri("mesh://cloud-a/fs/workspace/a.txt")
```

输出：

```ts
{
  node: "cloud-a",
  capability: "fs",
  mount: "workspace",
  path: "/a.txt"
}
```

---

# 3. Phase 3：Process / Job Capability

目标：不仅能读写远端文件，还能在远端执行任务。

不要直接只做 `exec`，应该抽象成 job。

## 3.1 Process Capability

能力：

```text
process.list
process.kill
process.inspect
```

## 3.2 Job Capability

能力：

```text
job.start
job.status
job.logs
job.cancel
job.artifacts
```

执行：

```bash
mesh run cloud-a "npm install && npm test"
```

返回：

```text
job-id
stdout stream
stderr stream
exit code
artifacts
```

内部模型：

```ts
interface Job {
  id: string
  nodeId: string
  command: string
  cwd?: string
  env?: Record<string, string>
  status: "running" | "success" | "failed" | "cancelled"
  startedAt: number
  finishedAt?: number
}
```

---

# 4. Phase 4：权限与安全边界

目标：让这个项目从玩具变成可信工具。

## 4.1 Policy Engine

每个 capability 都要经过 policy。

例如：

```yaml
policies:
  - subject: user:me
    node: cloud-a
    capability: fs
    mount: workspace
    actions:
      - read
      - write
    deny:
      - delete
```

## 4.2 Capability Token

不要让节点随便互相调用。

调用时带：

```text
session token
capability scope
expiry
caller identity
```

类似：

```text
can access cloud-a fs:workspace read/write for 1 hour
```

## 4.3 Secret Capability

MVP 不做复杂 vault，只做：

```bash
mesh secret add github:repo-read
mesh secret list
mesh secret revoke github:repo-read
```

使用：

```bash
mesh run cloud-a \
  --use-secret github:repo-read \
  "git clone git@github.com:xxx/private.git"
```

关键原则：

```text
secret 可以被能力使用，但不应该被调用方直接读取。
```

---

# 5. Phase 5：Mount 本地文件系统体验

目标：让远端文件真的像本地文件一样用。

## 5.1 Linux / macOS

做 FUSE adapter：

```bash
mesh mount cloud-a:/workspace ~/mesh/cloud-a
```

效果：

```bash
cd ~/mesh/cloud-a
ls
cat package.json
vim index.ts
code .
```

## 5.2 Windows

做 WinFsp adapter：

```text
M:\cloud-a\workspace
```

或者：

```text
\\mesh\cloud-a\workspace
```

## 5.3 设计原则

FUSE / WinFsp 只是 adapter，不是核心。

结构应该是：

```text
FUSE / WinFsp
      ↓
Mesh FS Client
      ↓
Mesh FS Protocol
      ↓
Remote FS Capability
```

---

# 6. Phase 6：大文件、缓存、Watch、同步

目标：让文件系统体验真正接近本地。

## 6.1 Streaming

支持：

```text
open
read range
write range
flush
close
```

用于：

```text
大文件
视频
模型文件
日志 tail
```

## 6.2 Cache

本地缓存：

```text
~/.mesh/cache/
```

缓存策略：

```text
read-through cache
write-through
write-back 可选
TTL
etag / mtime 校验
```

## 6.3 Watch

支持：

```bash
mesh fs watch cloud-a:/workspace
```

能力：

```text
file created
file changed
file deleted
file renamed
```

## 6.4 Sync

后续可以支持：

```bash
mesh sync ./project cloud-a:/workspace/project
```

但 sync 不要太早做，因为冲突处理很复杂。

---

# 7. Phase 7：Network Capability

目标：打通端口和服务访问。

能力：

```text
port.forward
port.expose
service.discover
```

命令：

```bash
mesh port forward cloud-a:3000 localhost:3000
mesh port expose local:5173 cloud-a:5173
```

典型场景：

```text
本地访问云端 dev server
云端访问本地 callback server
agent 调试远端 web app
```

---

# 8. Phase 8：Screen / Audio / Clipboard

目标：扩展到完整 computer capability。

## 8.1 Screen Capability

能力：

```text
screenshot
screen stream
mouse click
keyboard input
window list
```

URI：

```text
mesh://mac/screen/main
```

## 8.2 Clipboard Capability

能力：

```text
clipboard.read
clipboard.write
clipboard.watch
```

## 8.3 Audio Capability

能力：

```text
audio.capture
audio.play
audio.stream
```

这些都放后面，不要进入 MVP。

---

# 9. Phase 9：Web Console / GUI / IDE 插件

目标：提升可用性。

## 9.1 Web Console

功能：

```text
节点列表
文件浏览器
任务面板
日志查看
权限管理
secret 管理
拓扑视图
```

## 9.2 IDE 插件

VS Code 插件可以暴露：

```text
mesh://cloud-a/fs/workspace
```

类似 Remote SSH，但你的抽象更通用。

## 9.3 Agent Tool Integration

给 AI agent 暴露工具：

```text
mesh.fs.read
mesh.fs.write
mesh.job.run
mesh.port.forward
mesh.secret.use
```

这会是你项目很大的亮点。

---

# 10. 推荐 MVP 范围

不要一开始做太大。第一版建议只做：

```text
v0.1

- Node Agent
- Node Identity
- Direct WebSocket Transport
- FS Capability
- Job Capability
- CLI
- TypeScript SDK
- 简单 policy
```

v0.1 成功标准：

```bash
# 1. 启动两个节点
mesh agent start --name local
mesh agent start --name cloud-a

# 2. 连接节点
mesh node list

# 3. 操作远端文件
mesh fs ls cloud-a:/workspace
mesh fs cp ./a.txt cloud-a:/workspace/a.txt
mesh fs cat cloud-a:/workspace/a.txt

# 4. 执行远端任务
mesh run cloud-a "cat /workspace/a.txt"

# 5. 查看任务日志
mesh job logs <job-id>
```

---

# 11. 推荐仓库结构

```text
computer-mesh/
  packages/
    cli/
    agent/
    sdk/
    protocol/
    transport/
    capabilities/
      fs/
      job/
      process/
      secret/
      network/
    policy/
    uri/
    common/
  apps/
    web-console/
  docs/
    architecture.md
    protocol.md
    security.md
    roadmap.md
```

---

# 12. 最终路线图版本

```text
v0.1  FS + Job + CLI 最小闭环
v0.2  Policy + Secret + Audit
v0.3  Streaming + large file + watch
v0.4  FUSE / WinFsp mount
v0.5  Network port forwarding
v0.6  Web Console
v0.7  Agent tool integration
v0.8  Screen / clipboard
v0.9  Relay / NAT traversal / QUIC
v1.0  Stable protocol + plugin system + security hardening
```

最关键的一句话：

```text
先做 Capability Runtime，不要先做一堆具体功能。
```

因为只要这个内核稳了，后面文件、进程、端口、屏幕、声音、agent 工具调用，本质都是往 mesh 里挂 capability。

---

## 14. User

Time: 2026-04-27T09:57:52.457Z

我突然感觉，这是不是有点像云电脑或者remote kvm?

---

## 15. Assistant

Time: 2026-04-27T09:57:54.373Z

像，但**不是一类东西**。你这个方向更底层，也更“解耦”。

我直接帮你把三者掰开说清楚：

---

# 1️⃣ 先给一个直觉结论

```text
Remote KVM / 云电脑 = “远程操作一整台机器”

Computer Mesh = “跨机器调用能力（capability）”
```

---

# 2️⃣ Remote KVM / 云电脑在做什么

比如：

- entity["software","AnyDesk","remote desktop software"]  
- entity["software","TeamViewer","remote desktop software"]  
- 云电脑（各种 VDI / Cloud PC）

本质：

```text
你看到远端屏幕
你操作鼠标键盘
远端机器在执行
```

特点：

```text
- 粒度：整台机器
- 接口：屏幕 + 输入
- 抽象：人类操作
- 自动化能力：弱（要靠 UI automation）
```

---

# 3️⃣ 你的 Computer Mesh 在做什么

你这个是：

```text
mesh://node/fs/...
mesh://node/process/...
mesh://node/port/...
```

本质：

```text
直接调用远端能力，而不是“操作远端桌面”
```

特点：

```text
- 粒度：能力（filesystem / process / port / secret）
- 接口：API / CLI / SDK
- 抽象：程序调用
- 自动化能力：极强（agent 友好）
```

---

# 4️⃣ 对比一下（很关键）

| 维度 | Remote KVM / 云电脑 | Computer Mesh |
|------|------------------|--------------|
| 抽象层 | UI 层 | 能力层 |
| 操作方式 | 鼠标键盘 | API / CLI |
| 自动化 | 困难 | 天然支持 |
| 粒度 | 整机 | 细粒度能力 |
| 安全 | 全机暴露 | 可控 capability |
| 典型场景 | 人用 | 程序 / agent 用 |

---

# 5️⃣ 一个非常直观的例子

## Remote KVM 做这个：

```text
打开远端电脑
打开 terminal
cd /home
vim 文件
```

## Mesh 做这个：

```bash
mesh fs read cloud-a:/home/a.txt
mesh fs write cloud-a:/home/a.txt
```

甚至：

```bash
mesh run cloud-a "cat /home/a.txt"
```

👉 完全跳过 UI

---

# 6️⃣ 更关键的一点（你这个项目的“本质优势”）

Remote KVM：

```text
人 → 远端机器
```

Computer Mesh：

```text
机器 → 机器
agent → 多机器
```

例如：

```text
本地 agent：
→ 调 cloud GPU 节点跑训练
→ 调 NAS 拿数据
→ 调 mac 截图
→ 调云端部署服务
```

这是 Remote KVM 根本做不了的。

---

# 7️⃣ 但你可以“包含”它（很妙的一点）

你未来其实可以实现：

```text
ScreenCapability + InputCapability
```

这样：

```text
mesh://mac/screen/main
```

你就能做：

```text
一个 Remote Desktop
```

👉 但这是：

```text
Mesh 的一个 capability
```

而不是整个系统的核心。

---

# 8️⃣ 更准确的类比（我建议你对外这么讲）

不要说你在做 Remote KVM。

你更像是：

```text
“SSH + Tailscale + FUSE + Kubernetes + Capability System 的融合体”
```

或者更高级一点：

```text
“A capability-based distributed computer runtime”
```

---

# 9️⃣ 一句话帮你定调

👉 Remote KVM：

```text
让你“看到并操作另一台电脑”
```

👉 Computer Mesh：

```text
让所有电脑“像一个系统一样被调用”
```

---

# 10️⃣ 给你一个产品级洞察（很重要）

如果你走 Mesh 这条路：

```text
你的竞争对手不是 AnyDesk
```

而是：

- entity["software","SSH","secure shell protocol"]  
- entity["software","Tailscale","VPN mesh networking tool"]  
- entity["software","Kubernetes","container orchestration system"]  
- 各种 agent runtime

---

# 总结

你这个方向：

```text
不是“远程控制”
而是“分布式能力操作系统”
```

这俩听起来像，但**技术路线完全不同**。

---

如果你愿意，下一步我可以帮你做一件很关键的事：

👉 把这个项目**压缩成一个可以打爆 Hacker News / GitHub 的一句话定位 + README 结构**（这个对开源项目成败影响很大）。

---

## 16. User

Time: 2026-04-27T09:58:29.027Z

computer mesh 怎么体现出独特的竞争力？比如AI友好

---

## 17. Assistant

Time: 2026-04-27T09:58:29.905Z

你这个项目如果只是“多机互联”，竞争力其实不够——很容易被认为是 **SSH + Tailscale + rsync 的组合体**。  
真正能拉开差距的，是你刚刚提到的这个方向：

> 👉 **AI-native / Agent-native 的 Computer Mesh**

我帮你把“竞争力”拆成几层，从弱到强：

---

# 1️⃣ 普通版本（没差异的做法）

```text
远程文件 + 远程执行 + 端口转发
```

这会变成：

- entity["software","SSH","secure shell protocol"] + entity["software","rsync","file synchronization tool"]  
- + entity["software","Tailscale","mesh VPN tool"]  

👉 没有护城河

---

# 2️⃣ 真正的差异点：AI-native（核心）

你要做的是：

```text
Computer Mesh = Agent 可以直接调用的“现实世界接口层”
```

不是给人用的，是给 **LLM / agent 用的**。

---

# 3️⃣ 核心竞争力一：Capability ≠ API（而是可组合能力）

普通 API：

```text
getFile(path)
runCommand(cmd)
```

你的模型应该是：

```text
mesh://cloud-a/fs/workspace
mesh://gpu-node/job/run
mesh://mac/screen/main
```

👉 关键点：

```text
资源是“可组合的”
而不是一堆离散 API
```

例如 agent 可以：

```text
1. 在 cloud-a 找数据
2. 在 gpu-node 跑模型
3. 把结果写回 mac
```

---

# 4️⃣ 核心竞争力二：Agent Runtime（这是爆点）

你可以直接定义：

```text
Mesh 是 agent 的执行环境
```

而不是：

```text
agent 调用工具
```

区别很大：

## ❌ 传统 agent：

```text
LLM → tool → SSH → remote machine
```

## ✅ 你的：

```text
LLM → mesh runtime → 多节点能力调度
```

👉 你可以做：

```text
- 多节点并行执行
- 状态感知（哪个节点有 GPU / 数据）
- 自动选择执行位置
```

---

# 5️⃣ 核心竞争力三：Context-aware Compute

你可以让 Mesh 具备：

```text
“数据在哪，就在哪算”
```

比如：

```text
mesh.run({
  task: "train model",
  dataset: "mesh://nas/data/images",
})
```

系统自动：

```text
→ 找离数据最近的 GPU 节点
→ 调度执行
```

这就开始像：

- entity["software","Kubernetes","container orchestration system"]  
- + data locality  
- + agent reasoning  

👉 这是很强的差异点

---

# 6️⃣ 核心竞争力四：Secret-aware Execution

你前面问的 SecretCapability，这里就是关键：

```text
agent 可以做事，但拿不到 secret
```

例如：

```text
mesh.run(cloud-a, {
  cmd: "deploy",
  useSecret: "aws:prod"
})
```

👉 LLM 不会看到 AWS key  
👉 但可以完成部署

这是：

```text
AI automation 能进生产环境的关键门槛
```

---

# 7️⃣ 核心竞争力五：Deterministic + Observable

AI 系统最大问题是：

```text
不可控 / 不可复现
```

你可以反过来打：

```text
Mesh = 可追踪的执行图
```

例如：

```text
Task Graph:

local
 ├── read fs
 ├── call cloud-a
 │     └── run job
 └── call gpu-node
       └── run inference
```

然后：

```text
- 每一步都有 log
- 每一步有输入输出
- 可 replay
- 可 debug
```

👉 这对 agent 非常重要

---

# 8️⃣ 核心竞争力六：Tool ≠ Function，而是“环境”

现在很多 agent 工具是：

```json
{
  "name": "read_file",
  "params": { "path": "..." }
}
```

你可以变成：

```text
Agent 在 mesh 环境中运行
```

直接：

```bash
cat mesh://cloud-a/fs/workspace/a.txt
```

👉 这会让 agent 更像：

```text
真正的“操作系统用户”
```

而不是：

```text
调用一堆 function
```

---

# 9️⃣ 核心竞争力七：统一 Runtime（很重要）

你其实在做：

```text
“跨机器操作系统抽象”
```

统一：

```text
- 文件
- 进程
- 网络
- secret
- 设备
```

对 agent 来说：

```text
世界变成一个统一接口
```

这比：

```text
1000 个 tool schema
```

强太多。

---

# 🔟 一句话竞争力总结

你可以这样定义你的项目：

```text
Computer Mesh is an AI-native runtime that turns
distributed machines into a unified, capability-based system
for agents to operate on.
```

或者更狠一点：

```text
The missing operating system layer for AI agents.
```

---

# 11️⃣ 最关键建议（真的很关键）

你要刻意做这三件事：

### 1. 所有能力都可被 agent 调用

```text
fs / job / port / secret → tool-ready
```

### 2. 所有操作都有结构化语义

```text
不是 stdout 文本
而是 structured result
```

### 3. 所有执行都是可追踪的

```text
trace / span / graph
```

---

# 最后一句话

👉 如果你只是做：

```text
“远程操作工具”
```

你会输。

👉 如果你做的是：

```text
“AI 可以操作现实世界的 runtime”
```

这个方向是很有爆发力的。

---

如果你愿意，我下一步可以帮你：

👉 直接写一个 **README 首页（带 slogan + 架构图 + use case）**，这是开源项目能不能火的关键。

---

## 18. User

Time: 2026-04-27T09:59:18.878Z

ok。也帮忙一起想个名字

---

## 19. Assistant

Time: 2026-04-27T09:59:19.733Z

好，这一步很关键——名字会直接影响你这个项目是被当成“工具”，还是“新范式”。

我先给你**命名策略**，再给你一批我筛过的候选（分风格），最后给你我最推荐的 TOP 3。

---

# 一、命名策略（避免踩坑）

你的项目本质是：

```text
AI-native + distributed + capability-based + runtime
```

所以名字要满足：

### ✅ 应该有的感觉

- 像“系统/基础设施”，而不是小工具  
- 有一点抽象感（不要太具体，比如 file / ssh / sync）  
- 最好能让人联想到：
  - 网络（mesh / fabric / grid）
  - 控制（control / orchestrate）
  - 能力（capability / runtime）

---

### ❌ 不建议

```text
mesh-fs        （太窄）
remote-cli     （太工具）
super-ssh      （low level）
cloud-bridge   （太泛）
```

---

# 二、名字方向（我给你分了 4 类）

---

## 1️⃣ Mesh / 网络感（贴近你现在思路）

### 候选：

- **MeshOS**
- **OpenMesh Runtime**
- **MeshCore**
- **MeshLayer**
- **MeshFabric**
- **MeshControl**
- **MeshGrid**

👉 特点：

```text
稳、好理解，但略普通
```

---

## 2️⃣ Capability / Runtime 感（更高级）

### 候选：

- **Capra**（Capability + Infra）
- **CapMesh**
- **CapFlow**
- **CapNet**
- **CapStack**

👉 特点：

```text
更技术、更“新范式”
```

---

## 3️⃣ AI-native / Agent 感（最有差异化）

这是我最推荐的方向。

### 候选：

- **AgentMesh**
- **AgentFabric**
- **AgentGrid**
- **AgentOS**
- **AgentLayer**
- **MeshMind**
- **Operon**（操作单元的感觉）
- **Actra**（Action + Infra）

👉 特点：

```text
直接打 AI / agent 心智
```

---

## 4️⃣ 更“产品级 / 有记忆点”的名字（适合开源爆）

### 候选：

- **Nexa**（Next infra）
- **Orra**（Orchestrate + aura）
- **Zentra**（中心化控制感）
- **Ketra**
- **Vetra**
- **Omnix**（Omni + Unix）
- **Sysma**（System mesh）
- **Runa**（Run + infra）

👉 特点：

```text
好记，但需要 branding
```

---

# 三、我帮你筛的 TOP 3（最推荐）

## 🥇 1. AgentMesh

```text
AgentMesh
```

优点：

- 直接表达 AI-native
- 非常贴你核心卖点
- 好理解 + 好传播

一句话：

```text
AgentMesh = A capability network for AI agents
```

---

## 🥈 2. MeshOS

```text
MeshOS
```

优点：

- 很有“操作系统层”的感觉
- 容易被当成基础设施

一句话：

```text
MeshOS = The operating system for distributed computers
```

---

## 🥉 3. Capra

```text
Capra
```

优点：

- 短、独特、有技术味
- 可以解释为 Capability Runtime

一句话：

```text
Capra = Capability-based runtime for distributed machines
```

---

# 四、我个人建议（很直接）

如果你目标是：

### 👉 打 AI / agent 方向

选：

```text
AgentMesh
```

---

### 👉 打基础设施 / OS 方向

选：

```text
MeshOS
```

---

### 👉 想做一个“品牌感强”的长期项目

选：

```text
Capra
```

---

# 五、Bonus：一句话 Slogan（直接可用）

你可以配：

### AgentMesh：

```text
Turn distributed computers into a unified runtime for AI agents.
```

---

### MeshOS：

```text
A capability-based operating system for distributed machines.
```

---

### Capra：

```text
A runtime that lets machines safely operate each other.
```

---

# 最后一句建议

👉 不要选“描述性太强”的名字（比如 mesh-fs / remote-runner）

👉 要选一个**能装未来的名字**

---

如果你拍一个名字，我可以帮你直接把：

- README 首页
- 架构图（开源级）
- 第一个 demo（能上 Hacker News 那种）

一起帮你定出来 👍

---

## 20. User

Time: 2026-04-27T10:00:48.632Z

Operon 感觉不错，怎么融合

---

## 21. Assistant

Time: 2026-04-27T10:00:49.265Z

这个感觉很好，**Operon** 这个名字是有潜力做“范式级项目”的，但你现在要做的是：

> 👉 把 “Operon” 从一个名字 → 变成一个**有明确语义的系统模型**

否则它只是好听。

---

# 一、先把 Operon 语义“重新定义”（非常关键）

原本 entity["scientific_concept","Operon","genetic regulatory unit"] 在生物学里是：

```text
一组基因在一个调控单元下协同表达
```

你可以直接借这个隐喻，升级成：

```text
Operon = 一个跨机器的“能力执行单元”
```

---

## 👉 在你的系统里可以定义为：

```text
Operon = Capability + Context + Policy + Execution
```

或者更工程一点：

```text
Operon = 一个可调度、可组合、可追踪的分布式执行单元
```

---

# 二、把 Operon 融进你的整体架构

你现在的架构是：

```text
Node + Capability + Mesh
```

加入 Operon 之后，变成：

```text
Node
  ↓
Capability（fs / job / port / secret）
  ↓
Operon（组合能力执行）
  ↓
Mesh Runtime（调度 + 执行）
```

---

# 三、一个很关键的转变（这就是你项目的“灵魂”）

从：

```text
mesh.run("cloud-a", "npm install")
```

升级为：

```text
run Operon
```

---

## 举个例子（非常重要）

你定义一个 Operon：

```yaml
name: train-model

steps:
  - node: nas
    action: fs.read
    path: /data/images

  - node: gpu-node
    action: job.run
    command: python train.py

  - node: cloud-a
    action: fs.write
    path: /models/output
```

👉 这个 Operon 本质是：

```text
一个跨节点执行图
```

---

# 四、为什么这个设计很强

## 1️⃣ Agent 天然理解

LLM 不喜欢：

```text
一堆离散工具
```

但非常擅长：

```text
步骤 / 流程 / DAG
```

Operon 正好是：

```text
结构化执行计划
```

---

## 2️⃣ 可复用

```text
Operon = reusable unit
```

你可以：

```bash
mesh operon run train-model
mesh operon list
mesh operon inspect train-model
```

---

## 3️⃣ 可观察（这是杀手锏）

你可以做：

```text
Operon Execution Graph

local
 ├── fs.read
 ├── call gpu-node
 │     └── job.run
 └── call cloud-a
       └── fs.write
```

👉 每一步都有：

```text
- input
- output
- logs
- duration
- error
```

---

## 4️⃣ 可以做“调度器”

未来可以升级成：

```text
Operon Scheduler
```

自动决定：

```text
在哪个 node 执行
怎么并行
怎么 retry
怎么 fallback
```

---

# 五、和 Kubernetes 的关系（你可以类比）

你可以这样对外说：

```text
Kubernetes 管的是 container

Operon 管的是 capability execution
```

或者更狠一点：

```text
Kubernetes = container orchestration
Operon = capability orchestration
```

---

# 六、最终架构（推荐你这么表达）

```text
┌───────────────────────────────┐
│           Agent / CLI         │
└──────────────┬────────────────┘
               │
        Operon (Execution Unit)
               │
   ┌───────────┴───────────┐
   │                       │
Capability Layer      Policy Layer
(fs / job / ...)      (auth / secret)
   │                       │
   └───────────┬───────────┘
               │
         Mesh Runtime
               │
         Transport Layer
               │
            Nodes
```

---

# 七、你可以这样定义你的项目（很关键）

```text
Operon is an AI-native execution model for distributed computers.

It allows agents to compose, schedule, and execute capabilities
across machines as structured operons.
```

---

# 八、名字怎么融合（给你几个方案）

你可以选不同风格：

---

## 方案 A（最干净）

```text
Operon
```

子模块：

```text
Operon Mesh
Operon Runtime
Operon CLI
```

👉 最推荐

---

## 方案 B（稍微解释一下）

```text
Operon Mesh
```

👉 更容易理解，但稍微弱一点

---

## 方案 C（强调 runtime）

```text
Operon Runtime
```

👉 偏技术人

---

# 九、我给你的建议（很直接）

👉 如果你想做一个“能火的开源项目”：

```text
用 Operon 当核心概念
Mesh 当底层实现
```

也就是：

```text
Operon（用户看到的）
Mesh（系统内部）
```

---

# 🔟 一句话总结

👉 你现在不是在做：

```text
computer mesh
```

👉 而是在做：

```text
Operon = 分布式能力执行模型
```

---

如果你愿意，下一步我可以帮你做一个非常关键的东西：

👉 **README 首页（带 Operon 概念 + Demo + 架构图）**

这个会直接决定别人看到你项目时是：

```text
“哦，一个工具”
```

还是：

```text
“这东西有点东西”
```

---

## 22. User

Time: 2026-04-27T10:01:36.417Z

ok，搞

---

## 23. Assistant

Time: 2026-04-27T10:01:37.017Z

好，直接给你一个**可以开源首发用的 README 首页草稿**（偏 Hacker News / GitHub 爆款风格）。  
我已经帮你把 **Operon 概念 + AI-native + demo + 架构**都融合进去了，你可以直接复制用。

---

# 🚀 README（Operon）

```md
# Operon

> The missing execution model for AI agents.

Operon is an AI-native runtime that turns distributed computers into a unified, capability-based system.

Instead of treating machines as separate environments, Operon lets agents compose and execute **structured operons** — reusable, observable execution units that span across machines.

---

## ✨ What is an Operon?

Inspired by the concept of operons in biology, an **Operon** is a unit of coordinated execution.

In Operon:

- A **node** is any machine (local, cloud, container)
- A **capability** is something a node can do (filesystem, process, network, etc.)
- An **operon** is a composition of capabilities executed across nodes

```text
Operon = Capability + Context + Policy + Execution
```

---

## 🧠 Why Operon?

Today, AI agents interact with the real world through fragmented tools:

- SSH
- APIs
- File uploads
- Remote desktops

This leads to:

- ❌ Poor composability
- ❌ No execution trace
- ❌ Weak security boundaries
- ❌ Hard to automate reliably

Operon fixes this by introducing:

- ✅ A unified capability model
- ✅ Structured execution (not just commands)
- ✅ Built-in observability
- ✅ Secure, policy-driven execution

---

## ⚡ Example

Run a distributed workflow across machines:

```yaml
name: train-model

steps:
  - node: nas
    action: fs.read
    path: /data/images

  - node: gpu-node
    action: job.run
    command: python train.py

  - node: cloud-a
    action: fs.write
    path: /models/output
```

Run it:

```bash
operon run train-model.yaml
```

---

## 🔍 Execution Graph

Every operon produces a full execution trace:

```text
local
 ├── fs.read (nas)
 ├── job.run (gpu-node)
 └── fs.write (cloud-a)
```

Each step includes:

- input / output
- logs
- duration
- status

---

## 🧩 Capabilities

Operon exposes machines through capabilities:

```text
mesh://cloud-a/fs/workspace
mesh://gpu-node/job/run
mesh://mac/screen/main
```

Supported (initial):

- filesystem (read / write / list)
- job execution
- process control
- port forwarding

Planned:

- screen / input
- audio
- clipboard
- device access

---

## 🔐 Secure by Design

Operon enforces capability boundaries:

- Nodes only expose **explicit mounts**
- Secrets are **never directly accessible**
- Execution is **policy-controlled**
- Every action is **auditable**

---

## 🤖 AI-native

Operon is designed for agents, not humans.

Agents can:

- discover available capabilities
- compose operons dynamically
- execute across multiple nodes
- reason over execution results

```ts
await operon.run({
  steps: [
    { node: "cloud-a", action: "fs.read", path: "/data" },
    { node: "gpu-node", action: "job.run", command: "train.py" }
  ]
})
```

---

## 🏗 Architecture

```text
Agent / CLI
     ↓
Operon (Execution Unit)
     ↓
Capability Layer (fs / job / port / secret)
     ↓
Policy Layer
     ↓
Mesh Runtime
     ↓
Transport
     ↓
Nodes
```

---

## 🚧 Status

Operon is in early development.

Roadmap:

- [ ] Node runtime
- [ ] Filesystem capability
- [ ] Job execution
- [ ] CLI
- [ ] Policy system
- [ ] FUSE mount
- [ ] Agent integration

---

## 💡 Vision

Operon is not a remote control tool.

It is a new execution model where:

> Computers are no longer isolated machines,  
> but part of a unified, capability-based system  
> that AI agents can directly operate.

---

## ⭐️ Star this repo if this resonates

We're building the runtime layer for AI to interact with the real world.
```

---

# 🔥 我帮你点评一下这个版本的“杀伤力”

这个 README 已经具备：

### ✅ 1. 概念级（不是工具级）

不是：

```text
remote fs tool
```

而是：

```text
execution model
```

---

### ✅ 2. Operon 被“实体化”了

不再只是名字，而是：

```text
Operon = Execution Unit
```

---

### ✅ 3. AI-native 非常清晰

直接把你项目定位到：

```text
agent runtime
```

---

### ✅ 4. Demo 足够直观

那段 YAML：

```yaml
train-model
```

👉 非常关键（比一堆 API 更有传播力）

---

# 🔥 下一步我建议你做（非常关键）

如果你要让这个项目“真的起飞”，下一步是：

## 👉 做一个最小 demo（必须）

例如：

```bash
operon run examples/copy-file.yaml
```

内容：

```yaml
steps:
  - node: local
    action: fs.write
    path: /tmp/a.txt
    content: hello

  - node: cloud-a
    action: fs.read
    path: /tmp/a.txt
```

---

## 👉 再加一个 killer demo

```text
AI agent 自动：

- 找文件
- 跑任务
- 汇总结果
```

---

# 最后一刀（很关键）

你现在这个项目已经从：

```text
“computer mesh”
```

升级成：

```text
“Operon：AI execution model”
```

👉 这个级别是可以打大的。

---

如果你下一步要继续，我可以帮你直接：

- 设计 **operon YAML schema（可扩展）**
- 设计 **TS SDK（agent-friendly）**
- 或者给你写一个 **v0.1 最小实现骨架（monorepo + CLI + agent）**

你现在已经在一个很好的轨道上了 👍

---

## 24. User

Time: 2026-04-27T10:17:35.655Z

你建议我使用什么技术栈来做开发

---

## 25. Assistant

Time: 2026-04-27T10:17:39.012Z

建议用你熟悉的 **TypeScript / Node.js 做主体**，底层性能模块再用 **Rust**。不要一开始全 Rust，会拖慢产品迭代。

# 推荐技术栈

```text
Core / Protocol / SDK: TypeScript
CLI: TypeScript
Agent: TypeScript + 少量 Rust sidecar
Transport: WebSocket first，后续 QUIC
Config: YAML
Policy: JSON/YAML + OPA/Rego later
FS Mount: Rust FUSE / WinFsp adapter
Web Console: React + Vite
Package Manager: pnpm workspace
Build: tsup / tsdown / rollup
```

# 具体拆分

```text
packages/
  protocol/        # URI、RPC schema、message types
  sdk/             # TS SDK，给 agent / app / CLI 用
  cli/             # operon CLI
  agent/           # node agent
  transport-ws/    # WebSocket transport
  capabilities/
    fs/            # 文件能力
    job/           # 任务执行能力
    secret/        # secret 能力
    port/          # 端口转发能力
  policy/          # 权限判断
  config/          # YAML config loader
  common/
apps/
  web-console/     # 后续再做
crates/
  fuse-adapter/    # 后续 Rust 实现
```

# 为什么主体用 TypeScript

因为你的项目核心早期不是极致性能，而是：

```text
协议设计
Capability 抽象
Agent 调用
CLI 体验
AI integration
快速迭代
```

这些 TypeScript 更合适。

尤其你后面要做：

```text
AI agent tool schema
Vercel AI SDK
Web console
VS Code extension
JSON schema / YAML schema
```

TS 会非常顺手。

# 哪些地方适合 Rust

Rust 只放在这些边界：

```text
FUSE / WinFsp mount
高性能文件 streaming
端口转发高并发路径
本地系统能力采集
安全 sandbox runner
```

不要一开始把 agent 主体也写 Rust。否则你会在插件系统、schema、agent integration 上变慢。

# Transport 建议

MVP：

```text
WebSocket over TLS
```

原因：

```text
简单
好调试
浏览器/Web Console 可复用
Node.js 生态成熟
```

后续：

```text
QUIC
Relay
NAT traversal
WebRTC data channel
```

但这些不要 v0.1 做。

# RPC / Schema 建议

我建议用：

```text
JSON-RPC-like envelope + Zod schema
```

例如：

```ts
{
  id: "req_123",
  type: "request",
  capability: "fs",
  action: "readFile",
  target: "mesh://cloud-a/fs/workspace/a.txt",
  params: {}
}
```

TS 内部用：

```text
zod
```

后续可以导出：

```text
JSON Schema
OpenAPI-like docs
LLM tool schema
```

这点对 AI-native 很关键。

# CLI 建议

```text
commander 或 cac
tsx 开发
pkg / ncc / node SEA 后续打包
```

命令：

```bash
operon node list
operon fs ls cloud-a:/workspace
operon fs cat cloud-a:/workspace/a.txt
operon run cloud-a "npm test"
operon apply workflow.yaml
```

# 配置格式

用 YAML：

```yaml
node:
  name: cloud-a

capabilities:
  fs:
    mounts:
      - name: workspace
        path: /home/ubuntu/workspace
        permissions:
          read: true
          write: true
          delete: false

transport:
  listen: 0.0.0.0:7788
```

库：

```text
yaml
zod
```

# Job 执行

Node.js 里先用：

```text
child_process.spawn
```

注意必须设计：

```text
stdout/stderr streaming
exit code
cancel
timeout
cwd
env allowlist
secret injection
```

# 文件能力

MVP 用 Node.js：

```text
fs/promises
createReadStream
createWriteStream
```

后续 mount 再接：

```text
Rust FUSE adapter
```

# 数据存储

MVP：

```text
SQLite
```

用于：

```text
node registry
job records
audit logs
secret metadata
session records
```

Node.js 里推荐：

```text
better-sqlite3
```

# 加密 / 身份

MVP 可以用：

```text
Ed25519 keypair
mutual challenge handshake
session token
```

库可以选：

```text
@noble/ed25519
@noble/hashes
```

后续再做：

```text
OIDC
WebAuthn
TPM
mTLS
```

# 我最推荐的 v0.1 技术组合

```text
语言：TypeScript
运行时：Node.js 22+
包管理：pnpm workspace
CLI：commander
Schema：zod
配置：yaml
传输：ws
存储：better-sqlite3
日志：pino
测试：vitest
构建：tsup
Web：React + Vite
底层挂载：Rust later
```

# 一句话结论

```text
v0.1 用 TypeScript 快速把 Operon 的协议、CLI、agent、capability 模型跑通。
v0.2+ 再用 Rust 补 FUSE、WinFsp、streaming、port-forward 等性能/系统边界。
```

这个项目的核心竞争力不是底层语言，而是：

```text
AI-native capability runtime + 可追踪执行模型
```

---

