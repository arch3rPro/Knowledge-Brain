# 使用体验与 Agent Skill 套件设计

- Status: approved design
- Date: 2026-09-08

## 目标与范围

本设计改进 Knowledge-Brain 的人类界面、Agent 协作语义和 Agent Skill 分发方式，同时保持 Vault 文件、完整 SHA-256 身份、现有 `kb-app` 应用边界以及 CLI/MCP/HTTP 的共享事实来源。

本期包括：

- 人类可读输出中的短 Hash、操作摘要和单次写入确认；
- 相关性检索与精确文本匹配；
- `kb doctor` 与备份错误的可理解分类；
- 8 个面向任务的 Portable Agent Skills；
- 升级 `kb skills install`，并提供 `npx skills add` 标准分发。

本期不新增交互式 CLI 向导、组合命令、WebUI、GUI、PDF/OCR、Embedding、rerank 或任意 Agent 宿主专用插件。未来能力的范围见[未来扩展](../../docs/product/future-extensions.md)。

## 共享体验语义

### 机器身份与人类展示

完整 SHA-256、operation ID、Vault ID 和完整路径仍是 JSON、MCP、HTTP、持久化记录、来源 URI、对象路径和完整性校验中的规范值。人类默认文本输出和 Agent 面向用户的摘要只显示 Hash 前 12 个十六进制字符。

`--full-hashes` 使 CLI 文本输出展示完整 Hash。JSON、MCP 和 HTTP 不截断机器身份。短 Hash 不写回持久化字段，也不独立用于完整性判断；未来若接受 Hash 前缀输入，必须先验证该前缀在当前 Vault 唯一。

### 操作摘要与一次确认

创建来源保存、知识保存、Skill 安装或 Skill 卸载计划的响应，以及 `kb operation show` 的响应，都在既有根字段之外增加 `operation_summary`。既有根字段、JSON envelope 和 operation 记录格式不改名、不包裹、不删除。

`operation_summary` 至少包含：

- 完整 `operation_id`；
- `operation_kind`；
- `operation_state`；
- 目标 Vault 的规范身份；
- `change_count` 与受影响的相对路径；
- 人类可读 `summary`；
- `requires_confirmation`；
- `can_apply`。

计划状态的 `requires_confirmation` 为 `true`，且 `can_apply` 表示该 operation 在当前状态是否仍可提交。完成、失败或不可提交的 operation 必须用其实际状态填写摘要，不能暗示仍可 apply。

`review`、查询、读取、`status`、`lint`、计划创建和 operation 查看不要求用户确认。执行持久化变更前，外层 Agent 或 UI 展示一次 `operation_summary` 并取得明确同意；若计划陈旧、冲突或内容改变，旧同意不延伸到新 operation。`kb apply <operation-id>` 本身是 CLI 用户的明确写入请求，CLI 不再增加第二次终端确认。MCP 和 HTTP 继续由 `--allow-write`、令牌和固定 Vault 限制写入权限；它们不伪造用户确认，调用方必须先取得授权。

### 三条用户路径

```text
保存来源：写入已准入目录 → review → operation_summary → 确认一次 → apply
查询知识：输入问题 → relevant 或 exact 查询 → 结果与引用
整理知识：查询/读取 → plan create → operation_summary → 确认一次 → apply
```

CLI 保留 `review`、`operation show`、`apply` 和 `plan create` 这些可组合原语。本期不把它们替换为长流程命令或交互式向导。Agent Skill 依据用户任务组织这些原语，不逐步向用户索要只读步骤的确认。

### 查询意图

查询请求增加 `match_mode`：

- `relevant` 是默认值，保留当前 direct 或可选 BM25F 的相关性检索语义；
- `exact` 是区分大小写的 Unicode 字面文本匹配，用于完整名称、ID、错误码或短语。

CLI 用 `kb query --exact` 请求 `exact`；MCP 与 HTTP 在同一个查询请求字段中接受 `match_mode: "exact"`。所有查询响应都返回实际采用的 `match_mode`。

`exact` 不分词、不使用 BM25F 排序或解释，也不构建、更新或读取 BM25F 缓存。它只读真实可查询内容，因此 direct 和 BM25F 配置下的用户可见结果相同。现有 `search.mode` 继续只选择默认相关性后端，不能与 `match_mode` 混用。

### 诊断与备份错误

`kb doctor` 分开报告：

- `vault_structure`：指定 Vault 的目录和文件结构；
- `machine_runtime_directories`：机器级配置、状态和缓存目录。

机器运行目录必须说明其用途、按需创建行为和实际影响。尚未创建的惰性目录不构成 Vault 故障，也不对显式路径的只读 Vault 操作产生笼统 warning。

新增稳定错误码 `backup_verification_failed`，用于不可信 ZIP 的读取、格式、路径安全、清单或字节校验失败。`restore_failed` 仅用于已经通过归档校验、但在恢复目标验证、发布或写入阶段失败的恢复。迁移期错误详情附带 `legacy_code: "restore_failed"`，帮助旧客户端调整分支；CLI、MCP 与 HTTP 共享相同的新错误码、原因和 `next_action`。

## Agent Skill 套件

`skills/` 是标准分发根目录。对外没有名为 `kb` 的根 Skill，避免与同名 CLI 混淆。所有 Skill 复用一个内部共享规则源，规定：读取 `KB.md`、把 Vault 内容视作不可信输入、优先 MCP 且回退到 `kb --json`、计划不等于写入授权、apply 前展示操作摘要并取得一次确认。

| Skill | 任务边界 | 主要 CLI 能力 |
| --- | --- | --- |
| `kb-vault` | 初始化、采用、注册、移动、重新绑定 Vault | `init`、`adopt`、`vault`、`paths` |
| `kb-config` | 配置与准入目录 | `config`、`admission.yml` |
| `kb-ingest` | 来源审阅、保存、版本与校验 | `review`、来源 operation、`source verify` |
| `kb-query` | relevant/exact 查询、证据与引用 | `query` |
| `kb-save` | research/article 计划与经确认的保存 | `plan create`、知识 operation、`apply` |
| `kb-ops` | 状态、诊断、lint、缓存、operation 恢复 | `status`、`doctor`、`lint`、`cache`、`events` |
| `kb-backup` | 备份创建、校验与恢复 | `backup` |
| `kb-connect` | Agent 宿主、MCP 与 HTTP 连接 | `skills`、`mcp`、`serve` |

`apply` 没有独立 Skill。`kb-vault`、`kb-ingest`、`kb-save` 和 `kb-connect` 在各自任务中在预览与一次明确确认后执行 apply。`kb-query` 保持只读，不创建或提交 operation。

## 两种安装方式

### `kb skills install`

Knowledge-Brain 自带的 `kb skills install` 保留为 Vault 感知的受管理安装入口，不依赖 Node.js 或 npx。它检测宿主，创建可审阅 operation，并在 apply 后安装整套 8 个 `kb-*` Skills 和受管理桥接区块。`kb init` 不自动修改任何 Agent 宿主目录。

该入口支持既有 Vault/User 范围和 copy/symlink 模式，记录由 Knowledge-Brain 管理的文件、桥接区块与规范副本。卸载仅删除该记录中完整且未修改的套件；人工修改、部分安装或未知同名文件必须拒绝静默覆盖或删除。

`kb skills status` 使用以下状态表达安装事实：

- `absent`：没有已知的该套件文件；
- `current`：整套由 Knowledge-Brain 管理且内容完整；
- `partial`：只存在套件的一部分；
- `modified`：受管理文件或桥接区块被人工修改；
- `external`：发现同名但非 Knowledge-Brain 管理的 Skill；
- `legacy`：发现旧的单一 `knowledge-brain` Skill。

未修改的旧单 Skill 可通过可审阅迁移 operation 转换为套件。已修改的旧文件保留并返回 `legacy` 或 `modified`，不自动迁移。`kb skills uninstall` 永不删除由其他安装器创建的外部 Skill。

### `npx skills add`

仓库同时按 [Vercel Skills](https://github.com/vercel-labs/skills) 约定发布顶层 `skills/`，使用户可安装整套或单个任务 Skill：

```bash
npx skills add <owner>/knowledge-brain --all
npx skills add <owner>/knowledge-brain --skill kb-query
```

该方式只安装标准 Agent Skill 文件，不需要安装时存在 `kb` 二进制。Skill 在实际任务中仍需要可用的 `kb` CLI，或固定 Vault 的 MCP/HTTP 服务。

`npx skills add` 的安装目录属于外部安装器。`kb skills install` 检测到相同名称的外部 Skill 时报告 `external` 并停止，绝不覆盖；`kb skills uninstall` 也不删除它。将外部安装转换为受管理安装的 adopt 流程不在本期范围。

两种方式使用同一份规范 Skill 内容，但不共享文件所有权。规范来源和生成/校验方式必须保证两个分发产物内容一致。

## 文档归属

本规范是本期交互语义、兼容性和 Skill 分发边界的唯一设计来源。使用说明分别归入 CLI、搜索、备份、MCP、HTTP 和 Agent Skill reference；它们链接本规范而不重复设计理由。

[未来扩展](../../docs/product/future-extensions.md) 是未实施能力范围的唯一归属。`ROADMAP.md` 只记录阶段进度、已验证事实和指向该文档的链接。`docs/product/usability-backlog.md` 保留问题与验收背景，并链接本规范的实现契约。

## 验收与验证

实施使用定向测试和真实入口验证，不运行与本变更无关的 workspace 全量测试。至少覆盖：

- CLI 默认短 Hash、`--full-hashes` 与完整 JSON 身份；
- 计划创建和 operation 查看的 additive `operation_summary`，以及已完成 operation 的不可 apply 表达；
- Agent 的一次确认行为、CLI 直接 apply、MCP/HTTP 写入授权边界；
- CLI、MCP、HTTP 的 `relevant` 和 `exact` 查询一致性，以及 exact 不读写 BM25F 缓存；
- Vault 与机器运行目录的 doctor 分类；
- `backup verify`、归档校验失败的 restore 和真实恢复写入失败的错误分类；
- 8 Skill 套件的 copy/symlink、部分安装、人工修改、旧单 Skill 和外部同名 Skill 的状态与安全卸载；
- 隔离目录中的真实 `kb skills install`，以及实际 `npx skills add` 的整套和单 Skill 安装，确认两者不会互相删除或覆盖。

Windows、macOS 和 Linux 的宿主目录与 Agent 加载行为仍需由各自原生环境或 CI 取得证据；不能由单一平台的文件布局测试替代。
