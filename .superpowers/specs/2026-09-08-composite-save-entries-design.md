# 一次确认的保存入口设计

- Status: approved design
- Date: 2026-09-08
- Decision: [ADR-0018](../../docs/decisions/proposed/architecture/0018-composite-save-coordination.md)

## 用户层目标

用户不需要理解或操作“生成计划”“operation”“pending”或 operation ID。

当用户明确要求保存一项调研或知识变更时，Agent 或 UI 完成检查与准备后，只展示一次最终变更摘要。例如：

~~~
将保存
- 2 份已准入来源材料
- 新增 Wiki/articles/searxng-integration.md
- 更新 Wiki/index.md 与 Wiki/log.md

确认保存吗？
~~~

用户确认后，系统保存刚才摘要对应的内容，并返回完成结果。查询、读取、调研和准备不要求确认。

## 内部流程

“生成计划”是 Knowledge-Brain 的内部写入准备，不是用户步骤。它记录即将写入的内容、旧版本依据和恢复信息，使系统可以在写入前拒绝内容变化，并在中断后恢复。

~~~
Agent/UI 请求准备
        ↓
KB 内部创建可核对的 operation
        ↓
返回 change_summary + confirmation_token
        ↓
Agent/UI 向用户展示一次摘要并取得确认
        ↓
Agent/UI 提交 confirmation_token
        ↓
KB 对同一 operation 重新校验并写入
~~~

confirmation_token 是 Agent/UI 的机器字段。当前可由 operation ID 实现，但不得在普通用户提示、Skill 回复或 GUI 中要求用户复制、理解或输入它。

来源保存没有变化时，准备结果为 unchanged，不创建 token 或待提交 operation。

## 接口

### CLI

~~~
kb source save [--yes | --confirm <TOKEN>] [--vault <path-or-id>] [--json]
kb knowledge save <REQUEST.json> [--yes | --confirm <TOKEN>] [--vault <path-or-id>] [--json]
~~~

不带 --yes 或 --confirm 时，命令准备变更并返回摘要与 token，但不写入 Vault。

--confirm <TOKEN> 只提交该 token 对应的已准备操作。它是 Agent 或 UI 在用户确认后使用的机器参数，不是让用户执行的手工步骤。

--yes 适用于调用方已经取得用户授权、且不需要先展示此次预览的情况：命令内部准备并立即提交。它不能替代预览后的 --confirm，因为后者才能保证写入的是用户刚刚看到的同一份内容。

kb review、kb plan create、kb operation show 和 kb apply <operation-id> 不变，作为高级或人工操作入口。

### MCP

~~~
kb_source_save({ "apply": false })
kb_source_save({ "confirmation_token": "<TOKEN>" })
kb_source_save({ "apply": true })

kb_knowledge_save({ "request": { ... }, "apply": false })
kb_knowledge_save({ "request": { ... }, "confirmation_token": "<TOKEN>" })
kb_knowledge_save({ "request": { ... }, "apply": true })
~~~

三种模式互斥：

- 默认 apply: false：准备并返回摘要和 token；
- confirmation_token：提交已有的精确预览；
- apply: true：准备并立即提交。

后两种是写入请求，只有 kb mcp --allow-write 可用。调用 MCP 不等于用户确认；Agent 必须先展示摘要并获得一次确认。

### HTTP

~~~
POST /source/save
{ "apply": false }

POST /source/save
{ "confirmation_token": "<TOKEN>" }

POST /knowledge/save
{ "request": { ... }, "apply": false }

POST /knowledge/save
{ "request": { ... }, "confirmation_token": "<TOKEN>" }
~~~

HTTP 也接受 apply: true 作为一次调用内的准备并提交。confirmation_token 与 apply: true 是写入请求；没有 --allow-write 时返回 403，且不创建新的 operation。现有固定 Vault、Bearer token 与非 loopback 规则保持不变。

## 响应合同

组合入口使用新的响应对象，不修改旧原语的响应：

~~~json
{
  "phase": "awaiting_confirmation | applied | unchanged",
  "change_summary": {
    "change_count": 3,
    "affected_paths": ["Notes/SearXNG.md"],
    "summary": "Capture sources: 1 planned change."
  },
  "confirmation_token": "machine-only token or null",
  "result": null
}
~~~

- awaiting_confirmation：change_summary 描述精确准备结果；confirmation_token 非空；result 为 null。
- applied：change_summary 描述已完成结果；token 为 null；result 为已有 apply 结果。
- unchanged：没有可写内容；token 与 result 都为 null。

准备阶段的额外数据可在 preview 字段中提供给 Agent/UI，例如来源变化列表、知识 diff 和校验信息。普通用户只需要 change_summary。

提交失败后，稳定错误码保持原值，并在 error.details.confirmation_token 中保留 token。Agent/UI 可以报告失败、读取完成状态或重试同一 token；不得伪造已保存成功或静默重新准备。

## 一次确认的范围

一次确认覆盖用户已看到摘要中的全部写入。Agent 可以把多项已准备摘要合并成一个确认提示，并在确认后提交各自 token；这不承诺跨来源保存和知识保存的数据库式全成或全不发生。若其中一项失败，Agent 必须如实报告已完成与未完成的项目。

当前 source save 只捕获已经存在于 admission.yml 启用目录中的材料。若 Agent 自己要创作一份新的原始来源笔记，它必须在确认前放在 Vault 外部的草稿位置，或等待未来的显式 source-authoring 工作流；不能先直接写入 Vault 再把后续 capture 说成一次确认。

## 非目标

- 不要求用户输入 token、operation ID 或运行 apply；
- 不删除底层 operation、恢复、审计、锁或陈旧计划检查；
- 不新增交互式终端向导；
- 不把多个内部 operation 改造成新的跨目录全有或全无写入；
- 不实现 Agent 原始来源文件的草稿区、WebUI、GUI、PDF/OCR、Embedding 或 rerank。

## 验收与文档归属

实现使用真实 CLI、MCP、HTTP 路径和定向测试，至少覆盖：

- 默认准备只返回摘要/token、不写入；确认 token 写入与已确认预览一致；
- --yes / apply: true 的一调用准备并写入；
- 无变化来源的 unchanged 结果；
- token 错误、token 属于其他 Vault、陈旧 token 和失败后的 token 可追溯性；
- MCP/HTTP 的预览、确认 token、未授权拒绝和固定 Vault；
- Agent Skill 只面向用户展示摘要并只要求一次确认；
- 原有 review、plan create、apply 的兼容回归。

本文件拥有一次确认入口的产品与协议语义。命令、MCP、HTTP 与 Agent Skill reference 在实现后记录具体调用方式并链接本文件。
