# Fire 容器运行时 Makefile
.PHONY: all build test clean install uninstall fmt clippy doc release help

# 默认目标
all: build

# 变量定义
CARGO = cargo
INSTALL_PREFIX = /usr/local
BINARY_NAME = fire
TARGET_DIR = target
RELEASE_DIR = $(TARGET_DIR)/release
DEBUG_DIR = $(TARGET_DIR)/debug

# 帮助信息
help:
	@echo "Fire 容器运行时构建系统"
	@echo ""
	@echo "可用目标："
	@echo "  build       - 构建项目（调试模式）"
	@echo "  release     - 构建项目（发布模式）"
	@echo "  test        - 运行测试"
	@echo "  clean       - 清理构建产物"
	@echo "  install     - 安装到系统"
	@echo "  uninstall   - 从系统卸载"
	@echo "  fmt         - 格式化代码"
	@echo "  clippy      - 运行 clippy 检查"
	@echo "  doc         - 生成文档"
	@echo "  check       - 检查代码（不构建）"
	@echo "  deps        - 安装依赖"
	@echo "  example     - 运行示例"
	@echo "  benchmark   - 运行性能测试"
	@echo ""
	@echo "变量："
	@echo "  INSTALL_PREFIX=$(INSTALL_PREFIX)"
	@echo "  CARGO=$(CARGO)"

# 构建项目（调试模式）
build:
	@echo "构建 Fire 容器运行时..."
	$(CARGO) build

# 构建项目（发布模式）
release:
	@echo "构建 Fire 容器运行时（发布模式）..."
	$(CARGO) build --release

# 运行测试
test:
	@echo "运行测试..."
	$(CARGO) test

# 代码检查
check:
	@echo "检查代码..."
	$(CARGO) check

# 格式化代码
fmt:
	@echo "格式化代码..."
	$(CARGO) fmt

# 运行 clippy
clippy:
	@echo "运行 clippy 检查..."
	$(CARGO) clippy --all-targets --all-features -- -D warnings

# 生成文档
doc:
	@echo "生成文档..."
	$(CARGO) doc --no-deps --open

# 清理构建产物
clean:
	@echo "清理构建产物..."
	$(CARGO) clean
	rm -rf $(TARGET_DIR)

# 安装到系统
install: release
	@echo "安装 Fire 到 $(INSTALL_PREFIX)/bin/..."
	install -d $(INSTALL_PREFIX)/bin
	install -m 755 $(RELEASE_DIR)/$(BINARY_NAME) $(INSTALL_PREFIX)/bin/
	@echo "安装完成！"

# 从系统卸载
uninstall:
	@echo "卸载 Fire..."
	rm -f $(INSTALL_PREFIX)/bin/$(BINARY_NAME)
	@echo "卸载完成！"

# 安装依赖
deps:
	@echo "检查并安装依赖..."
	@which rustc > /dev/null || (echo "错误: 未找到 Rust 编译器，请安装 Rust" && exit 1)
	@which cargo > /dev/null || (echo "错误: 未找到 Cargo，请安装 Rust" && exit 1)
	$(CARGO) --version
	rustc --version

# 运行示例
example:
	@echo "运行示例..."
	@if [ ! -f $(DEBUG_DIR)/$(BINARY_NAME) ]; then \
		echo "请先运行 'make build'"; \
		exit 1; \
	fi
	@echo "创建测试容器..."
	./scripts/create_test_container.sh
	@echo "运行容器生命周期测试..."
	./scripts/test_lifecycle.sh

# 性能测试
benchmark:
	@echo "运行性能测试..."
	$(CARGO) bench

# 发布准备
prepare-release: clean fmt clippy test doc
	@echo "准备发布..."
	$(CARGO) build --release
	@echo "发布准备完成！"

# 开发模式（监视文件变化）
dev:
	@echo "启动开发模式..."
	@which cargo-watch > /dev/null || (echo "安装 cargo-watch: cargo install cargo-watch" && exit 1)
	cargo watch -x build

# 代码覆盖率
coverage:
	@echo "生成代码覆盖率报告..."
	@which cargo-tarpaulin > /dev/null || (echo "安装 cargo-tarpaulin: cargo install cargo-tarpaulin" && exit 1)
	$(CARGO) tarpaulin --out Html --output-dir coverage

# 安全审计
audit:
	@echo "运行安全审计..."
	@which cargo-audit > /dev/null || (echo "安装 cargo-audit: cargo install cargo-audit" && exit 1)
	$(CARGO) audit

# 检查更新
update:
	@echo "检查依赖更新..."
	$(CARGO) update

# 完整的 CI 检查
ci: deps fmt clippy test doc audit
	@echo "CI 检查完成！" 