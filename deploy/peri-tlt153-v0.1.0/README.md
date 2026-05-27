# peri TLT153-MiniEVM 部署包

## 简介

本部署包包含在 TLT153-MiniEVM (Allwinner T507) 开发板上运行的 peri agent。

## 系统要求

- TLT153-MiniEVM / SOM-TLT153 核心板
- Buildroot 2022.05 Linux 系统
- ARM Cortex-A7/A53 (armv7hf)
- GLIBC 2.34+
- 网络连接（用于 LLM API 通信）

## 文件说明

```
peri-tlt153-v0.1.0/
├── peri              # 主程序（ELF 32-bit ARM, hard-float）
├── lib/              # 依赖库
│   ├── libc.so.6     # GNU C Library
│   ├── libm.so.6     # Math library
│   └── libgcc_s.so.1 # GCC runtime
├── deploy.sh         # 部署脚本
└── README.md         # 本文件
```

## 快速部署

### 方式一：通过 SSH（推荐）

```bash
# 编辑 deploy.sh 或直接运行
scp -r . root@<板子IP>:/opt/peri
ssh root@<板子IP> "chmod +x /opt/peri/peri"
```

### 方式二：通过 SD 卡

```bash
mount /dev/sdX1 /mnt/sd
cp -r . /mnt/sd/opt/
umount /mnt/sd
# 插入开发板启动
```

## 运行

### 设置 API Key

```bash
# 方式 A: 环境变量（每次运行前）
export LD_LIBRARY_PATH=/opt/peri/lib:$LD_LIBRARY_PATH
export ANTHROPIC_API_KEY=sk-...   # 或 OPENAI_API_KEY
/opt/peri/peri --print "你好"

# 方式 B: 写入配置文件
cat >> /etc/profile.d/peri.sh << 'EOF'
export LD_LIBRARY_PATH=/opt/peri/lib:$LD_LIBRARY_PATH
export ANTHROPIC_API_KEY=sk-...
EOF
source /etc/profile.d/peri.sh
```

### 运行模式

```bash
# 单轮问答（最简单）
LD_LIBRARY_PATH=/opt/peri/lib /opt/peri/peri --print "你好"

# 指定模型
LD_LIBRARY_PATH=/opt/peri/lib /opt/peri/peri --print "你好" --model claude-sonnet-4-20250514

# 指定 OpenAI 兼容 API
export OPENAI_API_KEY=sk-...
export OPENAI_API_BASE=https://your-api.com/v1
LD_LIBRARY_PATH=/opt/peri/lib /opt/peri/peri --print "你好" --model gpt-4o
```

### TUI 模式（需要终端）

```bash
# 连接串口或 SSH 后
LD_LIBRARY_PATH=/opt/peri/lib /opt/peri/peri

# 或通过 -n 设置会话名
LD_LIBRARY_PATH=/opt/peri/lib /opt/peri/peri -n arm-test
```

## 依赖库说明

| 库 | 版本 | 说明 |
|----|------|------|
| libc.so.6 | 2.34+ | glibc，管理系统调用 |
| libm.so.6 | 2.34+ | 数学库 |
| libgcc_s.so.1 | 11.3.1 | GCC 运行时 |

这些库来自 Linaro gcc-linaro-11.3.1 工具链，与 Buildroot 2022.05 兼容。

## 故障排查

### "cannot execute binary file: Exec format error"
- 原因：架构不匹配或未设置执行权限
- 解决：`chmod +x /opt/peri/peri`

### "error while loading shared libraries: libXXX.so.X: cannot open"
- 原因：LD_LIBRARY_PATH 未设置或库版本不匹配
- 解决：确保 `LD_LIBRARY_PATH=/opt/peri/lib` 正确设置

### "error connecting to API"
- 原因：网络问题或 API Key 错误
- 解决：检查 `ping api.anthropic.com`，确认 API Key 正确

## 交叉编译说明

```bash
# 源码编译
git clone https://github.com/your/peri
cd peri
git checkout arm

# 工具链（来自 TLT153 资料盘）
# 4-软件资料/Linux/Tools/gcc-linaro-11.3.1-2022.06-x86_64_arm-linux-gnueabihf.tar.xz

TOOLCHAIN_ROOT=/opt/gcc-linaro-11.3.1-2022.06-x86_64_arm-linux-gnueabihf \
  ./build-arm.sh release
```

## 版本

- peri: 0.1.0
- 目标架构: arm-unknown-linux-gnueabihf
- 工具链: gcc-linaro-11.3.1-2022.06
- 编译日期: 2026-05-27
