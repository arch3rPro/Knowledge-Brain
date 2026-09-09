# ADR-0014: Durable operation events before SSE

- Status: accepted / implemented
- Class: architecture
- Spec: [Operation events and SSE](../../../../.superpowers/specs/2026-09-07-operation-events-sse.md)

## Problem

WebUI 和 GUI 需要观察计划与保存进度。若 HTTP 只发送内存心跳，进程退出或客户端重连后会丢失状态；若客户端通过重发 apply 获取进度，又会混淆观察和执行权限。

## Decision

先在机器本地 operation 目录保存 schema 化、有序且原子替换的事件日志，再由共享应用请求读取。计划、写入恢复记录、恢复动作和完成凭据在各自持久化边界追加事件。HTTP SSE 只轮询该应用请求，用连续事件 ID 和 `Last-Event-ID` 发送尚未见过的事件；终态发送后关闭连接。

事件不保存知识正文、diff、token 或任意路径。事件日志不是完成依据，也不代替 plan、result 或恢复记录。老 operation 没有日志时只合成当前状态，不在读取路径补写历史。

## Alternatives considered

**只发送内存进度。** 实现简单，但服务重启和客户端断线都会丢失 cursor 与历史。

**让 SSE 直接读取 progress/result 文件。** 这会把 operation 文件格式和 Vault 归属校验泄漏到网络适配器。

**客户端定时调用 operation show。** 可以判断最终结果，但不能区分恢复和写入进度，也没有稳定的断线续传位置。

**SSE 连接拥有并启动 apply。** 断线和重连会影响执行语义，并扩大只读订阅者权限。

## Consequences

- 观察、执行和恢复具有独立权限与生命周期。
- CLI、HTTP 及未来界面读取相同事件语义。
- 事件追加失败会遵循现有恢复路径，不允许未记录的 operation-owned 写入继续。
- 每次进度会增加机器本地原子写入；日志大小受 operation 工作上限约束。
- 浏览器行为、慢客户端负载、事件保留清理和跨机器复制仍需后续验证或设计。
