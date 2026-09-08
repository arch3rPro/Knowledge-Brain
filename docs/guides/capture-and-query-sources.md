# 保存与查询来源

前提：已有 Vault，且 `admission.yml` 中有启用的主题目录。下例使用用户创建的 `Notes/`，并假定 `kb` 已加入 PATH。

1. 在 `Notes/` 保存一份 Markdown 或 UTF-8 文本。
2. 检查并保存变化。已经明确要求本次写入时使用：

   ```bash
   kb source save --vault ./my-knowledge --yes --json
   ```

   命令只检查 `admission.yml` 中启用的目录，在同一次调用中准备并写入。没有变化时返回 `phase: unchanged`。原始主题文件不会被改写。

   如果 Agent 或 UI 尚未取得写入确认，省略 `--yes`。调用方展示返回的变更摘要，并在用户确认后提交同一个 token：

   ```bash
   kb source save --vault ./my-knowledge --confirm <confirmation-token> --json
   ```

   用户只确认最终写入，不需要看到 token。文件、准入或读取配置在确认前改变时，提交会被拒绝；调用方应重新准备并展示新摘要。

3. 查询并核对证据：

   ```bash
   kb query "关键词" --scope sources --vault ./my-knowledge
   kb source verify --vault ./my-knowledge --json
   ```

   逐项检查 verify 的状态，不要仅凭命令退出成功判断所有证据完好。来源查询使用已保存版本，不读取主题目录中的尚未保存改动。

4. 按需建立轻量目录：

   ```bash
   kb cache rebuild --vault ./my-knowledge
   kb query "关键词" --scope all --vault ./my-knowledge
   ```

   可以直接编辑 `Wiki/research/` 或 `Wiki/articles/` 的 Markdown；下一次查询立即看到修改，不要求先重建缓存。

## 来源发生变化后

重复 `kb source save --yes`，或由 Agent/UI 重复“准备摘要 → 一次确认 → 提交 token”。修改保留旧对象；删除只把来源记录标为不再存在；可能移动只是线索，不会自动合并两个来源身份。停用准入不会撤销以前保存的内容。

需要延后执行、脚本编排或逐项人工审核时，可以使用高级流程 `kb review → kb operation show → kb apply`。不要手工修改已审核计划来绕过确认时的状态核对。

## 保存中断后

查看 `kb status` 和 `.kb/runtime/source-pending.json` 中记录的操作 ID，重试 `kb apply <operation-id>`。不要删除标记或用户状态目录中的计划、进度文件。

如果提示文件被独立修改，先备份该人工改动并检查操作计划。只有确认需要恢复后，才手动恢复该文件到计划前或计划生成的内容，再重试。程序不会自行覆盖不认识的改动。

完成并清理中断操作后再复制或移动 Vault。已完成的 Vault 不依赖旧机器的注册表或缓存；未完成操作仍依赖旧机器上的恢复材料。
