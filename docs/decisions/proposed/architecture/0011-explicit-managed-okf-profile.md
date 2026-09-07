# 0011 — 显式标记 Knowledge-Brain 管理的 OKF 文档

- Status: proposed
- Class: architecture

## Problem

同一个 `Wiki/` 既允许人直接编写宽松的 OKF 文档，也包含由 Knowledge-Brain 生成和维护的文档。严格 Producer Profile 需要稳定、可解释的适用边界，且目录位置、作者或生成器都不能可靠表达后续写入所有权。

## Proposal

concept 文档只有在 frontmatter 包含 `kb.managed: true` 时才应用 Knowledge-Brain Producer Profile。未标记文档只应用 OKF v0.2 底线及其已出现可选字段的结构校验。

路径、`generated.by`、文件创建方式和当前编辑者不参与管理状态判断。未知 OKF 字段和 `kb` 扩展字段保持合法。

## Alternatives considered

**按目录推断。** `research/`、`articles/` 也允许人工创建内容，目录表示知识层次而不是写入所有权；推断会把人工文档意外纳入严格契约。

**按 `generated.by` 推断。** `generated` 记录当前内容如何产生，不授予未来修改权；人工编辑后的来源记录也可能保留生成信息。

**所有文档都执行严格规则。** 这会破坏 OKF 的开放性，并阻碍现有 Markdown 目录逐步采用 Knowledge-Brain。

## Acceptance criteria

- lint 只对 `kb.managed: true` 的 concept 应用严格字段要求。
- 同一路径下的未标记 concept 只需满足 OKF v0.2 底线。
- 管理状态不由路径或 `generated.by` 推断。
- 未知字段不导致失败。

## Risks

用户删除标记后文档会退出严格 Profile。知识保存流程必须在修改受管理文档前重新核对标记和计划快照；该写入保护属于后续 Stage 3 工作。

