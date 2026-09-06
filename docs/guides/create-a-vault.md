# 创建新 Vault

本指南的结果是一个最小 Vault 和一条显式准入记录。主题目录由你创建，Knowledge-Brain 不预设个人分类。

1. 构建当前 CLI。

   ```bash
   cargo build --release -p kb-cli
   ```

2. 初始化不存在或为空的目录。

   ```bash
   ./target/release/kb init ./knowledge
   ```

   如果目录已有内容，命令会停止并提示改用采用流程。初始化不会创建 `.git`。

3. 创建你自己的一级主题目录。

   ```bash
   mkdir ./knowledge/Notes
   ```

4. 先预览准入变化，再明确保存。

   ```bash
   ./target/release/kb config admission add notes Notes --vault ./knowledge
   ./target/release/kb config admission add notes Notes --vault ./knowledge --yes
   ```

5. 重新打开并验证落盘状态。

   ```bash
   ./target/release/kb config admission list --vault ./knowledge
   ./target/release/kb status --vault ./knowledge
   ./target/release/kb doctor --vault ./knowledge
   ```

目录选择、配置优先级和失败语义见[配置参考](../reference/configuration.md)；完整参数见[命令参考](../reference/commands.md)。
