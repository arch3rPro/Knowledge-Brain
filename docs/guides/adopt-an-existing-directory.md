# 采用已有目录

采用流程会为已有目录补充 Knowledge-Brain 框架文件，但不移动或改写已有文件，也不自动创建 Git 仓库。

1. 确认目录中没有 `.kb`、`admission.yml`、`KB.md`、`Wiki/index.md` 或 `Wiki/log.md` 所有权冲突。

2. 生成只读审核计划。

   ```bash
   ./target/release/kb adopt ./existing-notes --json
   ```

   保存输出中的 `data.operation_id`。此时重新检查目录，原文件应完全不变。计划将创建的 `admission.yml` 使用空 `directories`；已有一级目录不会被自动授权。

3. 检查已保存计划。

   ```bash
   ./target/release/kb operation show <OPERATION_ID> --json
   ```

4. 明确执行同一个计划。

   ```bash
   ./target/release/kb apply <OPERATION_ID> --json
   ```

   如果审核后任何已有路径发生变化，命令返回 `plan_stale`，不会添加框架文件。中断恢复只处理该操作进度明确记录、且内容仍匹配生成哈希的路径。

5. 重新打开并验证。

   ```bash
   ./target/release/kb status --vault ./existing-notes --json
   ./target/release/kb doctor --vault ./existing-notes --json
   ```

6. 按需创建准入记录。采用本身不会把已有目录加入知识库读取范围。

   ```bash
   ./target/release/kb config admission add notes Notes --vault ./existing-notes --yes
   ```

计划、注册和命令退出语义见[命令参考](../reference/commands.md)。
