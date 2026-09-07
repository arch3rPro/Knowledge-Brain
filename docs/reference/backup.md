# 备份、校验与恢复

## 命令

```text
kb backup create [--output <PATH.zip>] [--without-source-objects] [--vault <PATH_OR_ID>] [--json]
kb backup verify <PATH.zip> [--json]
kb backup restore <PATH.zip> --target <EMPTY_DIRECTORY> [--json]
```

`create` 默认在 Vault 的同级目录生成带 UTC 时间戳的 `.kb.zip` 文件；`--output` 相对当前工作目录解析。输出不能位于被备份的 Vault 内，也不会覆盖已有文件。存在待恢复的来源保存或知识保存时拒绝创建，以免保存一半新、一半旧的内容。

`verify` 和 `restore` 只需要归档路径，不要求原 Vault 或本机注册记录仍然存在。三种操作都由共享应用层实现，未来 MCP、HTTP、WebUI 和 GUI 不应自行读写 ZIP 或绕过相同校验。

## 包含与排除

完整备份包含：

- `admission.yml`、`KB.md`；
- `Wiki/`，包括来源记录和不可修改的原始来源对象；
- `admission.yml` 列出的全部一级主题目录，包括 `enabled: false` 的目录；
- `.kb/config.yml` 和 `.kb/schemas/`；
- 上述范围中的空目录、隐藏文件和未进入来源处理规则的普通文件。

备份不会套用来源的 `include/exclude` glob，因为备份目标是保存整个获准主题目录，不只是当前可提取文件。它始终排除 `.git`、`.kb/config.local.yml`、`.kb/cache/`、`.kb/runtime/`、用户级注册表、操作计划及其他本机状态。

`--without-source-objects` 还会排除 `Wiki/external-sources/.objects/`。这种归档保留来源记录，但无法独立证明或恢复全部历史来源字节；清单和结果中的 `complete_source_evidence` 明确为 `false`。它不能当作迁移前的完整证据备份。

## `manifest.json`

普通 ZIP 根目录中的 `manifest.json` 记录：

- 清单 `schema_version: v1.0`；
- Vault 的 `vault_schema_version` 和 `vault_id`；
- UTC 创建时间和程序版本；
- `complete_source_evidence`；
- 按可移植路径排序的目录列表；
- 每个文件的相对路径、字节数和小写 SHA-256。

`manifest.json` 是归档元数据，不会恢复到 Vault。Vault 文件仍是知识事实来源，ZIP 和清单不是运行时数据库。

## 校验边界

`verify` 把 ZIP 和清单都当作不可信输入。它要求 ZIP 条目与清单完全一致，并拒绝：

- 缺少、额外或重复条目；
- 绝对路径、`..`、反斜杠、符号链接、junction/reparse point 或特殊文件；
- Windows 保留名，以及跨平台大小写或 Unicode 冲突；
- 超出标准备份范围的文件；
- Vault 身份、准入目录、文件大小或 SHA-256 不一致；
- 精简备份中夹带原始来源对象。

校验成功只说明归档结构完整且每个字节符合清单，不表示 Markdown、YAML 或来源引用在语义上有效。恢复后仍可按需运行 `kb status`、`kb doctor`、`kb lint` 和 `kb source verify`。

## 恢复

目标必须不存在，或是一个真实的空目录；不会覆盖或合并已有内容。程序先完整校验归档，再解压到目标同级的私有临时目录，并在写入时再次核对大小和 SHA-256。所有文件通过后才把该目录放到目标位置。校验失败不会在目标留下部分 Vault。

恢复保留原 `vault_id`，但不复制或修改本机注册表。需要按 ID 选择时，显式运行：

```bash
kb vault register <RESTORED_DIRECTORY>
```

当前保证针对正常完成和进程内错误处理；尚未声称断电持久性、跨文件系统原子发布、加密、增量备份、云同步或多设备冲突合并。原生 Windows/Linux 跨系统恢复仍需要对应平台证据。
