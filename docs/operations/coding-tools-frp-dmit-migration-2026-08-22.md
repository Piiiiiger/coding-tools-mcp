# Coding Tools MCP：DMIT + FRP 迁移与维护记录（2026-08-22）

> 目的：记录 2026-08-22 这一轮从“本机 Cloudflare Named Tunnel”迁移到“本机 FRP → DMIT → Cloudflare”的完整结果、关键代码修改、踩坑、验收和回滚方法，避免后续维护时重复踩坑。
>
> **安全要求：本文不记录 FRP Token、Cloudflare Token、OAuth Secret、Tunnel credentials 内容。以后也不要把这些秘密补进文档或聊天。**

## 1. 最终结论

四个 Coding Tools MCP workspace 已全部迁移完成：

| Workspace | Workspace ID | 本地端口 | FRP 子域名 | 公网 URL | 当前状态 |
|---|---|---:|---|---|---|
| `ssh`（ChatGPT 中常用 `ac` 入口） | `b3918fc350f1494e808b089665e76037` | `28766` | `ssh-mcp` | `https://ssh-mcp.pigger.de5.net` | FRP |
| `Drone` | `a6fc2e2434094f499b99fa43ffdfd3b1` | `28767` | `drone-mcp` | `https://drone-mcp.pigger.de5.net` | FRP |
| `code` | `264787d41f7e4618ae678c1f85996922` | `28768` | `code` | `https://code.pigger.de5.net` | FRP |
| `grad` | `2cc7cd2ee1ba4ff5be868dd43ce63ea2` | `28769` | `grad` | `https://grad.pigger.de5.net` | FRP |

最终链路：

```text
ChatGPT / MCP Client
        |
        v
Cloudflare DNS + Proxy
        |
        v
Cloudflare Tunnel: dmit-frp-drone
        |  (connector 运行在 DMIT，不在本机)
        v
DMIT cloudflared
        |
        | HTTP -> 127.0.0.1:8080
        | Host 改写为 <subdomain>.frp.pigger.de5.net
        v
DMIT frps :8080 (HTTP vhost)
        |
        v
FRP control/data channel :7000
        |
        v
本机 Coding Tools managed frpc
        |
        v
127.0.0.1:28766 / 28767 / 28768 / 28769
```

最重要的变化不是“完全去掉 Cloudflare”，而是：

- **本机不再依赖 cloudflared 长连接。**
- 本机只需要 `frpc` 直连 DMIT `179.255.146.19:7000`。
- Cloudflare connector 放在公网 DMIT 上运行，避免本机 FlClash/TUN/代理重载把 Cloudflare Tunnel 一起打断。
- 公网域名保持不变，因此 ChatGPT 端无需更换 MCP URL。

## 2. DMIT FRP Server

DMIT SSH 别名：

```text
dmit
```

实际 FRP 控制服务器：

```text
179.255.146.19:7000
```

FRP Server：

```text
/usr/local/bin/frps
version: 0.61.2
```

systemd：

```text
/etc/systemd/system/frps.service
```

配置：

```text
/etc/frp/frps.toml
```

当前关键配置（Token 故意省略）：

```toml
bindAddr = "0.0.0.0"
bindPort = 7000

vhostHTTPPort = 8080

subDomainHost = "frp.pigger.de5.net"

auth.method = "token"
# auth.token = <SECRET - 不写入文档>
```

### 2.1 这一轮修复过的严重问题

最初 DMIT 上的以下文件曾被错误写成“字面量 `\n`”，而不是真换行：

- `/etc/frp/frps.toml`
- `/etc/systemd/system/frps.service`
- Nginx 的 FRP vhost 配置

症状包括：

```text
systemd: Bad message
frps inactive
```

已经全部重写为真实换行格式。以后如果看到 `Bad message`，第一件事应检查配置文件是不是被某个脚本写成了单行的 `\n` 文本。

### 2.2 `subDomainHost` 不要误改

当前仍然是：

```text
subDomainHost = frp.pigger.de5.net
```

而公网 URL 是：

```text
ssh-mcp.pigger.de5.net
drone-mcp.pigger.de5.net
code.pigger.de5.net
grad.pigger.de5.net
```

这是**有意设计**，不是配置错误。

Cloudflare Tunnel 在 DMIT 上把公网 Host 改写为：

```text
ssh-mcp.frp.pigger.de5.net
drone-mcp.frp.pigger.de5.net
code.frp.pigger.de5.net
grad.frp.pigger.de5.net
```

然后再送给 `frps:8080`，因此 FRP 的 subdomain 路由可以正常匹配。

**不要为了“看起来一致”直接把 `subDomainHost` 改成 `pigger.de5.net`。** 这会改变 FRP 的路由语义，需要同步修改其它层，不能单独动。

## 3. DMIT Cloudflare Tunnel

新建的专用 Tunnel：

```text
name: dmit-frp-drone
id:   4e394a3b-43c8-43af-9eef-dd841bffbf56
```

虽然名字里仍然有 `drone`，现在它已经承载四个 Coding Tools MCP hostname。

运行配置：

```text
/etc/cloudflared/drone-frp.yml
```

Tunnel runtime credentials：

```text
/root/.cloudflared/4e394a3b-43c8-43af-9eef-dd841bffbf56.json
```

**这个 JSON 是秘密，不要复制到文档、Git 或聊天。**

systemd：

```text
/etc/systemd/system/cloudflared-drone-frp.service
```

当前服务要求：

```text
frps: active + enabled
cloudflared-drone-frp.service: active + enabled
```

当前 ingress：

```yaml
tunnel: 4e394a3b-43c8-43af-9eef-dd841bffbf56
credentials-file: /root/.cloudflared/4e394a3b-43c8-43af-9eef-dd841bffbf56.json

protocol: http2

ingress:
  - hostname: drone-mcp.pigger.de5.net
    service: http://127.0.0.1:8080
    originRequest:
      httpHostHeader: drone-mcp.frp.pigger.de5.net

  - hostname: ssh-mcp.pigger.de5.net
    service: http://127.0.0.1:8080
    originRequest:
      httpHostHeader: ssh-mcp.frp.pigger.de5.net

  - hostname: code.pigger.de5.net
    service: http://127.0.0.1:8080
    originRequest:
      httpHostHeader: code.frp.pigger.de5.net

  - hostname: grad.pigger.de5.net
    service: http://127.0.0.1:8080
    originRequest:
      httpHostHeader: grad.frp.pigger.de5.net

  - service: http_status:404
```

### 3.1 DNS

四条 Cloudflare DNS 记录现在都应为 CNAME，且保持 **Proxied / 橙云**：

```text
ssh-mcp.pigger.de5.net   -> 4e394a3b-43c8-43af-9eef-dd841bffbf56.cfargotunnel.com
drone-mcp.pigger.de5.net -> 4e394a3b-43c8-43af-9eef-dd841bffbf56.cfargotunnel.com
code.pigger.de5.net      -> 4e394a3b-43c8-43af-9eef-dd841bffbf56.cfargotunnel.com
grad.pigger.de5.net      -> 4e394a3b-43c8-43af-9eef-dd841bffbf56.cfargotunnel.com
```

这一轮尝试使用已有 `cert.pem` 自动修改 DNS 时，Cloudflare 返回：

```text
Authentication error
```

原因是已有凭据可以管理 Tunnel，但没有 `DNS:Edit` 权限。因此三条后续 DNS 是手动修改的。

如果以后希望脚本自动切 DNS，需要单独准备最小权限的 Cloudflare API Token，例如只给目标 Zone 的 DNS Edit 权限；不要复用范围过大的全局凭据。

### 3.2 Cloudflare 管理证书处理

创建 Tunnel 时临时复制到 DMIT 的 Cloudflare 管理 `cert.pem` 已在完成后删除。

DMIT 只保留新 Tunnel 自身运行所需的 credentials JSON。

## 4. 本机 Coding Tools FRP 配置

配置文件：

```text
~/.config/coding-tools-mcp-desktop/data/profiles.json
```

全局 FRP Profile：

```text
name: Dmit
id: 3c891d43090e4d43b820170c33538fa0
server: 179.255.146.19
server_port: 7000
```

全局 FRP Token 存在 Secret Store / `app_secrets.frp_profile_token` 中。

**不要把 Token 值写进普通 workspace 字段、Git、文档或聊天。**

当前 workspace 状态：

```text
ssh:
  type=frp
  public_url=https://ssh-mcp.pigger.de5.net
  frp_server=179.255.146.19
  frp_subdomain=ssh-mcp
  frp_profile_id=3c891d43090e4d43b820170c33538fa0
  use_proxy=false

Drone:
  type=frp
  public_url=https://drone-mcp.pigger.de5.net
  frp_server=179.255.146.19
  frp_subdomain=drone-mcp
  frp_profile_id=<empty, current legacy/manual-compatible state>
  use_proxy=false

code:
  type=frp
  public_url=https://code.pigger.de5.net
  frp_server=179.255.146.19
  frp_subdomain=code
  frp_profile_id=3c891d43090e4d43b820170c33538fa0
  use_proxy=false

grad:
  type=frp
  public_url=https://grad.pigger.de5.net
  frp_server=179.255.146.19
  frp_subdomain=grad
  frp_profile_id=3c891d43090e4d43b820170c33538fa0
  use_proxy=false
```

### 4.1 Drone 的 `frp_profile_id` 为空是当前已验证可工作的状态

Drone 是最先迁移的 workspace，当时采用 manual/legacy 方式，因此 `frp_profile_id` 仍为空，但 workspace 自己已经有可用 FRP Secret，且整条链路已经实际验证通过。

不要仅仅为了“统一字段”在没有备份/验收的情况下修改它。

以后如果要把 Drone 也标准化为全局 `Dmit` Profile，应先备份 `profiles.json`，再通过 UI 选择 `Dmit`，随后重新验证 OAuth + FRP；不要直接手改 Secret 值。

## 5. FlClash / TUN：必须保留 DMIT 直连

本机 FRP 控制连接必须直连：

```text
179.255.146.19:7000
```

FlClash 中已经加入并持久化：

```text
IP-CIDR,179.255.146.19/32,DIRECT,no-resolve
```

同时所有 Coding Tools FRP workspace 都必须：

```text
use_proxy=false
```

原因非常重要：`179.255.146.19` 本身也是 FlClash 中 “洛杉矶-Dmit” 节点的服务器 IP。如果 FRP 请求被 TUN 再交给代理组，有可能出现自绕/套娃路径：

```text
frpc -> FlClash -> DMIT proxy node -> DMIT:7000
```

之前的症状就是 `frpc` 长时间停在：

```text
try to connect to server...
```

而没有进入 Token 鉴权阶段。

因此以后排障时：

1. 先确认 workspace 的 `use_proxy=false`。
2. 再确认 FlClash 的 DMIT IP DIRECT 规则仍存在且优先级高于 MATCH/兜底规则。
3. 然后再怀疑 Token。

如果是 Token 错，一般已经建立 TCP 连接，日志会进入登录/鉴权失败；如果一直只有 `try to connect to server...`，优先查网络/TUN/路由。

## 6. Coding Tools 代码修改：公网 URL 与 FRP 控制地址必须解耦

这是这一轮最重要的软件修复。

### 6.1 原错误行为

旧逻辑在 FRP 模式下会用：

```text
https://<frp_subdomain>.<frp_server>
```

自动生成公网 URL。

当：

```text
frp_server = 179.255.146.19
frp_subdomain = drone-mcp
```

时，会错误得到：

```text
https://drone-mcp.179.255.146.19
```

结果 MCP 虽然能通过 FRP，但 OAuth 返回：

```text
Www-Authenticate:
Bearer resource_metadata="https://drone-mcp.179.255.146.19/..."
```

这会导致 ChatGPT OAuth/MCP 发现流程错误。

### 6.2 已修改的位置

#### `src-tauri/src/tunnel/frp/mod.rs`

`frp_public_url()` 现在：

- 如果 workspace 已明确配置 `public_url`，优先使用它。
- 只有旧 profile 没有 `public_url` 时，才保留 `https://subdomain.server` 作为 legacy fallback。

#### `src-tauri/src/workspace/model.rs`

`computed_public_url()` 也做了相同修复。

这一处特别重要，因为 MCP listener / OAuth / health checker 最终会走 `WorkspaceProfile::effective_public_url()`。

**只修 `frp_public_url()` 不够。** 这一轮第一次修完 FRP 模块以后，OAuth 仍然返回 IP，就是因为 `computed_public_url()` 还有第二处旧逻辑。

### 6.3 回归测试

新增/确认的关键测试包括：

```text
workspace::model::computed_public_url_tests::frp_prefers_explicit_public_url
workspace::model::computed_public_url_tests::frp_keeps_legacy_generated_url_fallback
tunnel::frp::tests::frp_public_url_prefers_configured_public_url
tunnel::frp::tests::frp_public_url_keeps_legacy_generated_fallback
```

这四个测试已经通过。

以后改 FRP URL 逻辑时，必须保证两个原则同时成立：

```text
显式 public_url > 自动推导 URL
旧配置无 public_url 时仍保持兼容 fallback
```

## 7. Coding Tools 新增的安全重启/迁移辅助入口

文件：

```text
src-tauri/src/lib.rs
```

新增环境变量：

```text
CODING_TOOLS_AUTOSTART_WORKSPACES
```

用途：在一次性迁移/远程恢复时，让新 Coding Tools 进程启动后自动调用正式 `start_runtime()` 流程拉起指定 workspace。

本轮用它完成了不中途依赖 `grad` 的自动 handoff。

四个 workspace ID：

```text
b3918fc350f1494e808b089665e76037  # ssh
a6fc2e2434094f499b99fa43ffdfd3b1  # Drone
264787d41f7e4618ae678c1f85996922  # code
2cc7cd2ee1ba4ff5be868dd43ce63ea2  # grad
```

示意：

```bash
CODING_TOOLS_AUTOSTART_WORKSPACES="b3918fc350f1494e808b089665e76037,a6fc2e2434094f499b99fa43ffdfd3b1,264787d41f7e4618ae678c1f85996922,2cc7cd2ee1ba4ff5be868dd43ce63ea2" \
  /home/pigger/code/coding-tools-mcp/src-tauri/target/release/coding-tools-mcp-desktop
```

注意：

- 这是迁移/恢复辅助功能，不是建议长期在 shell profile 中永久设置的环境变量。
- 如果旧实例还占着 `28766-28769`，新实例会遇到端口冲突；正式 handoff 应先确保旧监听释放。
- 正常日常启动不需要设置这个变量。

## 8. **非常重要：Tauri 构建方式**

### 8.1 不要再用裸 `cargo build --release` 部署桌面应用

这一轮曾执行：

```bash
cargo build --release
```

然后重启 Coding Tools，出现：

```text
Could not connect to localhost: Connection refused
```

原因：这没有经过完整 Tauri build 的前端资源 / custom protocol 打包流程，生成的二进制不能作为当前桌面应用的正式部署产物使用。

### 8.2 推荐构建命令

只需要生成可运行 release 二进制，不需要打 AppImage/DEB/RPM：

```bash
cd /home/pigger/code/coding-tools-mcp
npm run tauri -- build --no-bundle
```

正式产物：

```text
/home/pigger/code/coding-tools-mcp/src-tauri/target/release/coding-tools-mcp-desktop
```

需要完整安装包时可以：

```bash
npm run desktop:build
```

但是本轮完整 build 在最后 AppImage 阶段出现过：

```text
failed to run linuxdeploy
```

**这不等于主程序编译失败。** 当日志已经出现：

```text
Built application at: .../target/release/coding-tools-mcp-desktop
```

说明主二进制已经生成，失败的是后面的 Linux bundle 打包。

以后不要看到 `linuxdeploy` 失败就重新改代码或用裸 `cargo build` 顶替。

## 9. managed FRP 的运行文件与日志

Coding Tools 正常管理的每个 FRP client 配置在：

```text
~/.config/coding-tools-mcp-desktop/frpc/<workspace-id>/frpc.toml
```

日志在：

```text
~/.config/coding-tools-mcp-desktop/logs/<workspace-id>/frpc-mcp.log
```

正常启动必须看到：

```text
login to server success
proxy added
start proxy success
```

本轮最终四个 managed `frpc` 都出现了上述日志。

注意：Coding Tools 的 FRP route 与本地 MCP runtime 生命周期相关。

如果某个 workspace 本地端口没有启动，例如 `28767` 没监听，那么 Coding Tools 关闭该 workspace 的 `frpc` 是正常行为，不要误判成 FRP 掉线。

排查顺序应先问：

```text
本地 MCP runtime 是否运行？
    -> 是：再查 frpc
    -> 否：frpc 没有 route 是正常的
```

## 10. 旧本机 Cloudflare Tunnel

旧容器：

```text
coding-tools-mcp-cloudflared-host
image: cloudflare/cloudflared:latest
```

旧共享 Named Tunnel：

```text
name: coding-tools-mcp
id: bb980d0d-1de0-4b78-af7a-498322d22452
```

旧 tunnel 的远程 ingress 曾同时指向：

```text
ssh-mcp.pigger.de5.net   -> http://127.0.0.1:28766
drone-mcp.pigger.de5.net -> http://127.0.0.1:28767
code.pigger.de5.net      -> http://127.0.0.1:28768
grad.pigger.de5.net      -> http://127.0.0.1:28769
```

当前容器状态：

```text
Exited (0)
```

**目前是“停止但保留”，没有删除。**

这是有意保留的应急回滚手段。等新架构稳定足够长时间后，再决定是否彻底删除旧容器/旧 Tunnel。

## 11. 回滚方案

### 11.1 DMIT/FRP 整体故障时的最快应急回滚

如果 DMIT 整机不可用，当前 FRP + DMIT Cloudflare connector 会一起失效。此时可临时恢复旧本机 Cloudflare Tunnel：

1. 启动旧容器：

   ```bash
   docker start coding-tools-mcp-cloudflared-host
   ```

2. 在 Cloudflare DNS 把需要回滚的 hostname CNAME 改回旧 Tunnel：

   ```text
   bb980d0d-1de0-4b78-af7a-498322d22452.cfargotunnel.com
   ```

3. 保持橙云 `Proxied`。

4. 确认本机对应 `28766-28769` 端口仍在监听。

旧 cloudflared 是独立容器，只要远程 ingress 仍有效，它可以直接访问这些 localhost 端口；紧急情况下不必第一时间把 Coding Tools profile 全部改回 `cloudflare`。

### 11.2 本轮 workspace 配置备份

在迁移 `ssh/code/grad` 前已经备份：

```text
~/.config/coding-tools-mcp-desktop/data/profiles.json.bak-frp-all-20260822-201824
```

这个备份对应的是：

- Drone 已经是 FRP。
- ssh/code/grad 还没有正式切 FRP。

**以后不要直接覆盖当前 `profiles.json`。** 如果已经有新的 workspace/secret 修改，应先 diff，再选择性恢复字段。

## 12. Nginx 的位置：目前不是 MCP 主链路必需项

DMIT 上此前修过 Nginx `frp-vhost`，可把 HTTP Host 请求转给 `127.0.0.1:8080`。

但当前四个 MCP 的正式公网链路是：

```text
cloudflared -> 127.0.0.1:8080 -> frps
```

即 Cloudflare Tunnel **直接访问 frps 的 HTTP vhost 端口**，不经过 Nginx。

因此以后排查四个 MCP 时，不要把 Nginx 当作首要依赖；Nginx 当前更多是辅助/历史 HTTP vhost 入口。

## 13. 标准验收流程

### 13.1 本机

确认 Coding Tools 一个主进程负责四个端口：

```text
28766 ssh
28767 Drone
28768 code
28769 grad
```

检查每个 `frpc-mcp.log`：

```text
login to server success
start proxy success
```

### 13.2 DMIT

```bash
ssh dmit 'systemctl is-active frps; systemctl is-enabled frps'
ssh dmit 'systemctl is-active cloudflared-drone-frp.service; systemctl is-enabled cloudflared-drone-frp.service'
```

预期均为：

```text
active
enabled
```

FRP 内层验证示例：

```bash
ssh dmit 'curl -i -H "Host: grad.frp.pigger.de5.net" http://127.0.0.1:8080/mcp'
```

预期：

```text
HTTP/1.1 401 Unauthorized
```

### 13.3 公网

```bash
curl -i https://ssh-mcp.pigger.de5.net/mcp
curl -i https://drone-mcp.pigger.de5.net/mcp
curl -i https://code.pigger.de5.net/mcp
curl -i https://grad.pigger.de5.net/mcp
```

未带 Authorization 时，**401 是正确结果**，不是故障。

同时必须检查：

```text
Www-Authenticate: Bearer resource_metadata="https://<正确公网域名>/.well-known/oauth-protected-resource/mcp"
```

绝对不能再次出现：

```text
https://<subdomain>.179.255.146.19/...
```

### 13.4 ChatGPT 侧

最终不能只以 curl 为准，还要实际调用连接器：

- `ac/ssh` 应返回 `/home/pigger/code/ssh` workspace。
- `Drone` 应返回 `/home/pigger/code/Drone` workspace。
- `code` 应返回 `/home/pigger/code` workspace。
- `grad` 应返回 `/home/pigger` workspace。

这一轮四条已经实际验证通过。

## 14. 常见故障快速判断

### A. `frpc` 一直停在 `try to connect to server...`

优先检查：

```text
use_proxy 是否 false
FlClash 是否仍有 179.255.146.19/32 DIRECT
本机是否能 TCP 连接 DMIT:7000
```

不要一上来就重置 Token。

### B. `login to server success`，但公网 404

检查：

- FRP proxy 是否 `start proxy success`。
- cloudflared 的 `httpHostHeader` 是否对应 `<subdomain>.frp.pigger.de5.net`。
- DNS CNAME 是否指向当前 `4e394a3b-...cfargotunnel.com`。
- workspace 本地 MCP runtime 是否仍然在监听。

### C. 公网 502

按顺序：

```text
Cloudflare DNS
-> DMIT cloudflared service
-> DMIT frps
-> FRP route
-> 本机 frpc
-> 本地 MCP 端口
```

### D. 公网能返回 401，但 OAuth 地址里出现 IP

这通常意味着：

- 运行的是修复前旧 Coding Tools 二进制；或
- `computed_public_url()` / `frp_public_url()` 回归；或
- workspace 的显式 `public_url` 被清空。

先确认运行二进制和 profile，不要改 Cloudflare DNS。

### E. Coding Tools 窗口出现 `Could not connect to localhost`

先确认是否误用：

```text
cargo build --release
```

正确重新构建：

```bash
cd /home/pigger/code/coding-tools-mcp
npm run tauri -- build --no-bundle
```

## 15. 工作树注意事项

记录本文时，`coding-tools-mcp` 工作树中还存在其它之前遗留/并行工作的未提交修改，例如 OAuth、Actions、listener、Cloudflare 相关代码，以及部分 GitNexus/CLAUDE 文件状态变化。

因此：

> **不要为了回滚本次 FRP 修改直接执行 `git reset --hard`、`git checkout .` 或删除整个工作树改动。**

本轮确认直接相关的核心源代码文件主要是：

```text
src-tauri/src/tunnel/frp/mod.rs
src-tauri/src/workspace/model.rs
src-tauri/src/lib.rs
```

但提交前仍应逐文件审阅 diff，区分本轮 FRP 修改与之前尚未提交的其它工作。

## 16. 安全与维护约定

1. FRP Token 只存在服务端配置与 Coding Tools Secret Store，不进 Git。
2. Cloudflare Tunnel credentials JSON 不进 Git，不复制到本机无关目录。
3. DMIT 上用于创建 Tunnel 的临时 Cloudflare `cert.pem` 已删除，不要为了排障随意复制管理证书到服务器。
4. 如果需要查看 FRP Token，只在受信任终端本地读取，不要贴到聊天：

   ```bash
   ssh dmit "grep '^auth.token' /etc/frp/frps.toml"
   ```

5. 不要给 `frpc` 打开 Coding Tools 的“使用网络代理”。FRP 控制通道应该直连 DMIT。
6. 不要删除旧 `coding-tools-mcp-cloudflared-host` 容器，直到新架构经过足够长时间稳定运行并明确决定取消回滚通道。

## 17. 当前最终基线

截至 2026-08-22 本轮收口：

```text
DMIT frps:                         active
DMIT cloudflared-drone-frp:       active + enabled
本机 Coding Tools:                 单一新版进程
ssh managed frpc:                 success
Drone managed frpc:               success
code managed frpc:                success
grad managed frpc:                success
四个公网 /mcp:                     HTTP 401（正确）
四个 OAuth resource_metadata:      正确公网域名
ChatGPT ac/ssh:                    可调用
ChatGPT Drone:                     可调用
ChatGPT code:                      可调用
ChatGPT grad:                      可调用
旧 coding-tools cloudflared 容器:  stopped / retained for rollback
```

如果后续有人重新设计此架构，必须先理解这份记录再动以下三处：

```text
FRP 控制地址
公网 OAuth URL
Cloudflare ingress Host override
```

这三者是**不同概念**，本轮大部分故障都来自把它们错误地绑成同一个地址。
