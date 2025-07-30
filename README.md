# Fire 容器运行时

Fire 是一个基于 Rust 实现的 OCI 兼容容器运行时，支持完整的容器生命周期管理。项目实现了核心的容器运行时功能，包括命名空间隔离、根文件系统挂载、资源控制、安全强化等特性，为容器化应用提供可靠的运行环境。

## 功能特性

- ✅ **OCI 兼容（核心）**：已实现 Spec 解析、namespace 隔离、rootfs 挂载与 cgroup 资源限制
- ✅ **容器生命周期管理**：支持 create / start / run / kill / delete / state 等命令
- ✅ **cgroups 资源限制**：实现 cpu、memory、blkio、pids 等控制器
- ✅ **Namespace 隔离**：完整实现所有7种 Linux namespace 类型（PID、Mount、Network、IPC、UTS、User、Cgroup）
- ✅ **Rootfs 挂载与 pivot_root**：完整实现挂载管道、设备节点创建、文件系统切根操作
- ✅ **安全功能**：完整实现 seccomp 过滤器，基础 SELinux 支持
- ✅ **模块化架构 & 中文日志**：Rust 2021，错误信息和日志友好
- 🚧 **Hook 系统**：基础框架完成，执行逻辑待实现
- 🚧 **信号处理**：信号映射完成，进程间信号转发待完善
- ⚠️ **网络管理**：veth、bridge、iptables 等网络功能尚未实现
- ⚠️ **存储管理**：overlayfs、卷挂载等高级存储功能尚未实现

### 项目完成度

Fire 容器运行时项目已完成约 **70%** 的核心功能：

- **核心容器运行时**：完整实现（100%）
- **命名空间隔离**：完整实现（100%）
- **根文件系统管理**：完整实现（100%）
- **资源控制**：完整实现（100%）
- **安全功能**：基本实现（80%）
- **Hook 系统**：部分实现（50%）
- **信号处理**：部分实现（60%）
- **网络管理**：未实现（0%）
- **存储管理**：未实现（0%）

## 安装构建

### 前置要求

- Rust 1.70.0 或更高版本
- Linux 内核（需启用 cgroups、namespaces）
- 具有 **root 权限**（挂载、cgroup 写入等操作需要）

### 构建

```bash
git clone https://github.com/wu-eee/fire
cd fire
cargo build --release
```

### 安装

```bash
cargo install --path .
```

## 使用方法

### 基本命令

```bash
# 查看帮助
fire --help

# 列出所有容器
fire ps

# 创建容器
fire create <container-id> [bundle-path]

# 启动容器
fire start <container-id>

# 查看容器状态
fire state <container-id>

# 向容器发送信号
fire kill <container-id> [--signal <signal>]

# 删除容器
fire delete <container-id> [--force]

# 一键运行容器（创建+启动）
fire run <container-id> [bundle-path]
```

### 示例

```bash
# 创建并启动一个测试容器
fire create mycontainer /path/to/bundle
fire start mycontainer

# 查看容器状态
fire state mycontainer

# 停止并删除容器
fire kill mycontainer
fire delete mycontainer

# 或者一键运行
fire run mycontainer /path/to/bundle
```

## 配置文件

Fire 使用标准的 OCI 配置文件格式 (`config.json`)。示例配置文件：

```json
{
  "ociVersion": "1.0.0",
  "process": {
    "terminal": false,
    "user": {
      "uid": 0,
      "gid": 0
    },
    "args": ["/bin/sh", "-c", "echo 'Hello from Fire!'"],
    "env": [
      "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
      "TERM=xterm"
    ],
    "cwd": "/"
  },
  "root": {
    "path": "rootfs",
    "readonly": true
  },
  "hostname": "fire-container",
  "linux": {
    "namespaces": [
      {"type": "pid"},
      {"type": "network"},
      {"type": "ipc"},
      {"type": "uts"},
      {"type": "mount"},
      {"type": "user"},
      {"type": "cgroup"}
    ],
    "resources": {
      "cpu": {
        "quota": 100000,
        "period": 100000
      },
      "memory": {
        "limit": 134217728
      }
    }
  }
}
```

## 目录结构

```
fire/
├── src/
│   ├── commands/          # 命令实现
│   │   ├── create.rs      # 创建容器
│   │   ├── start.rs       # 启动容器
│   │   ├── kill.rs        # 终止容器
│   │   ├── delete.rs      # 删除容器
│   │   ├── state.rs       # 状态查询
│   │   ├── run.rs         # 运行容器
│   │   └── ps.rs          # 列出容器
│   ├── container/         # 容器管理
│   ├── runtime/           # 运行时管理
│   ├── namespace/         # 命名空间隔离
│   ├── rootfs/            # 根文件系统挂载
│   ├── cgroup/            # 资源控制组
│   ├── security/          # 安全功能（seccomp、SELinux）
│   ├── signal/            # 信号处理
│   ├── hooks/             # Hook 系统
│   ├── errors.rs          # 错误处理
│   └── main.rs            # 主程序
├── oci/                   # OCI 规范实现
├── docs/                  # 文档
├── test_bundle/           # 测试容器包
└── target/                # 构建输出
```

## 技术架构

### 核心组件

1. **命令层**：处理用户输入，实现各种容器操作命令
2. **运行时层**：管理容器生命周期，维护容器状态
3. **容器层**：封装容器相关操作和状态管理
4. **OCI 层**：实现 OCI 规范的数据结构和序列化
5. **命名空间层**：实现所有7种 Linux 命名空间隔离
6. **根文件系统层**：实现挂载管道、pivot_root 和设备节点创建
7. **资源控制层**：实现 cgroup v1/v2 资源限制和管理
8. **安全层**：实现 seccomp 过滤器和 SELinux 支持

### 关键特性

- **类型安全**：使用 Rust 的类型系统保证内存安全
- **错误处理**：统一的错误处理机制，详细的错误信息
- **日志系统**：结构化日志，支持不同级别的日志输出
- **模块化**：清晰的模块边界，易于维护和扩展
- **OCI 兼容**：严格遵循 OCI 运行时规范实现
- **资源隔离**：完整的命名空间和 cgroup 隔离机制
- **安全强化**：多层次的安全保护机制

## 状态管理

容器状态存储在 `~/.fire/<container-id>/state.json` 文件中，包含：

- 容器 ID
- 当前状态（created/running/stopped）
- 进程 PID
- Bundle 路径
- 注解信息

## 开发指南

### 添加新命令

1. 在 `src/commands/` 目录下创建新的命令文件
2. 实现 `Command` trait
3. 在 `src/commands/mod.rs` 中注册新命令
4. 在 `src/main.rs` 中添加命令行参数解析

### 扩展功能

- **网络管理**：实现 veth、bridge、iptables 等网络功能，完善网络命名空间配置
- **存储管理**：支持 overlayfs、卷挂载等高级存储功能和存储驱动
- **Hook 系统**：完善 prestart、poststart、poststop 等 Hook 执行逻辑
- **信号处理**：完善容器进程间信号转发和处理机制
- **安全功能**：增强 SELinux、AppArmor 支持，完善 seccomp 过滤器
- **监控集成**：添加 Prometheus 指标和健康检查

## 故障排除

### 常见问题

1. **权限错误**：确保有足够的权限创建目录和文件
2. **配置错误**：检查 `config.json` 文件格式是否正确
3. **路径问题**：确保 bundle 路径和 rootfs 路径存在

### 日志调试

```bash
# 查看详细日志
RUST_LOG=debug fire <command>

# 查看错误信息
fire <command> 2>&1 | grep ERROR
```

## 贡献指南

欢迎贡献代码！请遵循以下步骤：

1. Fork 项目
2. 创建功能分支
3. 提交更改
4. 创建 Pull Request

## 许可证

本项目采用 GNU General Public License v2.0 许可证 - 详见 [LICENSE](LICENSE) 文件。

## 致谢

- [OCI 运行时规范](https://github.com/opencontainers/runtime-spec)
- [Rust 编程语言](https://rust-lang.org/)
- [clap 命令行解析库](https://docs.rs/clap/)
- [serde 序列化库](https://serde.rs/)
