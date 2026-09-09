# ADR-0013: Optional fixed-Vault HTTP adapter

- Status: accepted / implemented
- Class: architecture
- Spec: [HTTP API design](../../../../.superpowers/specs/2026-09-07-http-api.md)

## Problem

WebUI、GUI 和局域网客户端需要一个不解析 CLI 文本的应用入口，但常规本地文件操作不应依赖 daemon。网络入口还可能意外扩大为任意路径访问、跨 Vault operation 执行或默认暴露写入权限。

## Decision

在唯一的 `kb` 可执行文件中提供按需启动的 `kb serve`。服务固定到启动时解析的一个 Vault ID，只公开选定的 typed `AppRequest`；查看和应用 operation 时在应用层复核其 Vault ID。默认监听回环地址且只读。非回环监听或开放 apply 都要求 token 文件；配置 token 后所有路由验证 Bearer token。

首个实现使用明文 HTTP，不提供宽松 CORS、TLS、daemon、自启动或占位 SSE。局域网使用仅面向受信任网络或用户管理的 TLS 反向代理。SSE 等到可持久化 operation 事件契约存在后再实现。

## Alternatives considered

**让 GUI 直接读写 Vault。** 这会复制路径、锁、schema、恢复和权限规则，破坏一个应用层的决定。

**所有 CLI 命令都经过常驻服务。** 这会让离线本地文件操作依赖 daemon 生命周期和端口可用性。

**默认生成并保存服务 token。** 自动持久化凭据会引入生命周期、权限和找回策略；显式 token 文件更适合当前按需服务，也不会把 secret 写入 Vault。

**首版同时提供 SSE。** 没有持久化进度事件时只能发送无意义心跳或重复执行查询，无法保证断线续传语义。

## Consequences

- CLI、HTTP 和未来界面共享业务结果、锁、计划与恢复规则。
- HTTP 请求不能选择任意 Vault、归档或文件路径；跨 Vault operation ID 被拒绝。
- 回环读取可零配置启动，局域网和写入需要明确凭据与权限。
- Bearer token 不提供传输加密；非受信任网络需要外部 TLS。
- TLS、浏览器 CSRF/Origin 策略、SSE 和原生跨平台网络证据仍需后续阶段完成。
