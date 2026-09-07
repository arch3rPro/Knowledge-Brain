# 保存与查询来源

前提：已有 Vault，且 `admission.yml` 中有启用的主题目录。下例使用用户创建的 `Notes/`，并假定 `kb` 已加入 PATH。

1. 在 `Notes/` 保存一份 Markdown 或 UTF-8 文本。
2. 查看变化及将保存的内容：

   ```bash
   kb review --vault ./my-knowledge --json
   kb operation show <operation-id> --json
   ```

   `<operation-id>` 使用 review 结果中的 ID。`null` 表示没有需要保存的变化。检查目标文件、来源版本和日志变更；原始主题文件不会被改写。

3. 明确执行计划：

   ```bash
   kb apply <operation-id> --json
   ```

   如果文件、准入或读取配置在 review 后改变，先解决变化，再生成新计划。不要手工修改已审核计划来绕过校验。

4. 查询并核对证据：

   ```bash
   kb query "关键词" --scope sources --vault ./my-knowledge
   kb source verify --vault ./my-knowledge --json
   ```

   逐项检查 verify 的状态，不要仅凭命令退出成功判断所有证据完好。来源查询使用已保存版本，不读取主题目录中的尚未保存改动。

5. 按需建立轻量目录：

   ```bash
   kb cache rebuild --vault ./my-knowledge
   kb query "关键词" --scope all --vault ./my-knowledge
   ```

   可以直接编辑 `Wiki/research/` 或 `Wiki/articles/` 的 Markdown；下一次查询立即看到修改，不要求先重建缓存。

## 来源发生变化后

重复 `review → operation show → apply`。修改保留旧对象；删除只把来源记录标为不再存在；可能移动只是线索，不会自动合并两个来源身份。停用准入不会撤销以前保存的内容。

## 保存中断后

查看 `kb status` 和 `.kb/runtime/source-pending.json` 中记录的操作 ID，重试 `kb apply <operation-id>`。不要删除标记或用户状态目录中的计划、进度文件。

如果提示文件被独立修改，先备份该人工改动并检查操作计划。只有确认需要恢复后，才手动恢复该文件到计划前或计划生成的内容，再重试。程序不会自行覆盖不认识的改动。

完成并清理中断操作后再复制或移动 Vault。已完成的 Vault 不依赖旧机器的注册表或缓存；未完成操作仍依赖旧机器上的恢复材料。
