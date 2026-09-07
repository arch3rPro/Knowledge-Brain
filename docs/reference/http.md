# HTTP 参考

`kb serve` 是可选的 HTTP 适配器。CLI 和文件工作流不依赖常驻服务；HTTP 只把请求转换为共享应用层操作，不自行读取或修改 Vault。

## 启动

```text
kb serve [--bind <IP:PORT>] [--token-file <PATH>] [--allow-write] [--vault <PATH_OR_ID>]
```

默认监听 `127.0.0.1:9432`，只开放读取、检查和计划创建。端口 `0` 可用于让操作系统选择空闲端口。服务启动后在 stdout 输出一行 JSON，其中包含实际地址、Vault ID、Vault 根目录、鉴权状态和写入策略。

非回环地址或 `--allow-write` 必须同时提供 `--token-file`。文件应包含一个不带 `Bearer` 前缀的非空 token；去除首尾空白后，token 不能包含空白字符，文件不能是链接、目录或超过 4096 字节。请求使用：

```text
Authorization: Bearer <token>
```

配置 token 后，每个路由都要求鉴权。启动时选中的 Vault 固定到其稳定 ID；请求不能改用其他 Vault，也不能查看或应用其他 Vault 的 operation ID。

## 路由

| 方法与路径 | 请求体 | 用途 |
| --- | --- | --- |
| `GET /capabilities` | 无 | 能力及当前 HTTP 权限 |
| `GET /status` | 无 | Vault 事实状态 |
| `GET /doctor` | 无 | 独立诊断结果 |
| `POST /query` | `SearchRequest` JSON | 查询 Wiki 或来源 |
| `POST /lint` | 无 | 检查 Wiki 结构 |
| `POST /review` | 无 | 审查准入来源并创建计划 |
| `POST /plans` | `KnowledgePlanRequest` JSON | 创建知识保存计划 |
| `GET /operations/{id}` | 无 | 查看计划或完成结果 |
| `POST /operations/{id}/apply` | 无 | 应用计划，仅 `--allow-write` |

`SearchRequest` 的字段为 `query`、`scope`（`wiki`、`sources` 或 `all`）、`limit` 和 `strict_backend`。知识计划请求见[知识计划参考](knowledge-plans.md)。JSON 请求体上限为 1 MiB。

成功与失败都使用和 CLI `--json` 相同的 `schema_version: v1.0` 信封。常见 HTTP 映射是：鉴权缺失 `401`、权限不足 `403`、Vault/operation 不存在 `404`、陈旧计划或恢复冲突 `409`、请求或领域校验失败 `400`、内部 I/O 失败 `500`。客户端仍应以稳定的 `error.code` 判断业务原因。

## 网络边界

当前实现是明文 HTTP，不提供 TLS，也不发送宽松 CORS 头。局域网监听是明确允许的可选项，但只适用于受信任网络，或放在用户管理的 TLS 反向代理之后。把 token 放进 URL、Vault 配置或版本库会泄露凭据。

服务收到 Ctrl-C 后停止接受新连接并结束。当前没有 daemon 安装、后台自启动或 SSE；后续只有在 operation 具备可持久化进度事件和断线续传语义后才增加 SSE。
