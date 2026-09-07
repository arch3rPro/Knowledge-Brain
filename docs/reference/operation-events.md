# Operation 事件参考

Knowledge-Brain 把操作进度保存在机器本地用户状态目录的 `operations/<operation_id>/events.json`。它与 plan、恢复记录和 result 同属执行状态，不是 Vault 知识，不进入备份。

## 事件模型

事件 ID 是 operation 内从 1 开始、连续递增的整数。事件按实际持久化顺序出现：

| `kind` | 含义 | 当前尝试结束 |
| --- | --- | --- |
| `planned` | 计划已保存，可供审查 | 否 |
| `applying` | 一次 apply 已开始 | 否 |
| `progress` | 对应恢复记录已保存，相关步骤已完成 | 否 |
| `recovering` | 正在恢复上次中断的 operation-owned 写入 | 否 |
| `applied` | result 已持久化，知识变更已完成 | 是 |
| `failed` | 本次 apply 返回错误 | 是；同一计划仍可能按其错误语义重试 |

每项包含 `id`、`kind`、单行 `message`、RFC 3339 `recorded_at`，可计量时还包含 `completed` 和 `total`。事件不保存来源正文、生成的 Markdown、diff、token 或任意目标路径。

相同 kind 和进度的连续状态只保存一次。大型写入集合记录第一步、约 1% 的里程碑和完成，而不是每个文件都追加事件。完成后的同一 operation ID 再次 apply 会返回已有 result，不追加重复完成事件。老版本创建且没有事件文件的 operation 在读取时得到一个内存合成的当前状态；读取不会为了补历史而修改用户状态。

事件日志损坏、ID 不连续、身份不符或超出上限时不会被服务。operation 的业务结果仍由 plan/result 和稳定错误码决定；事件用于观察，不代替完成凭据或恢复记录。

## SSE

```text
GET /operations/<OPERATION_ID>/events
Accept: text/event-stream
Last-Event-ID: <EVENT_ID>
```

SSE 复用 `kb serve` 的 token 和固定 Vault 边界。事件名称为 `operation`，SSE `id` 等于持久事件 ID，`data` 是事件 JSON。省略 `Last-Event-ID` 会从头发送；提供后只发送更大的 ID。无效、零值或比日志更新的 cursor 在开始流式响应前返回普通 JSON `invalid_config`。

流在 `applied` 或 `failed` 发送后关闭。活动 operation 每 250 ms 读取一次共享应用报告；15 秒无事件时可以发送仅用于维持连接的 SSE comment。客户端断线只取消订阅，不取消或重试 apply。多个订阅者不拥有 operation。

当前未验证浏览器 `EventSource`、慢客户端负载、原生 Windows/Linux 网络行为或跨机器事件同步。
