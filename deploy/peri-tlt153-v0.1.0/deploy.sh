#!/bin/bash
#
# peri TLT153-MiniEVM 部署脚本
#
# 用法:
#   ./deploy.sh                      # 交互式部署
#   ./deploy.sh /dev/sdX            # 部署到 SD 卡设备
#   ./deploy.sh root@192.168.1.100  # 通过 SSH 部署
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PERI_BIN="$SCRIPT_DIR/peri"
LIB_DIR="$SCRIPT_DIR/lib"
TARGET_DIR="/opt/peri"
LIB_TARGET_DIR="$TARGET_DIR/lib"

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()    { echo -e "${GREEN}[INFO]${NC} $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
usage()   { echo "用法: $0 [目标]"
           echo ""
           echo "目标:"
           echo "  root@<ip>         通过 SSH 部署到远程设备"
           echo "  /dev/sdX          部署到 SD 卡（需要 root）"
           echo "  local             仅本地打包，不部署"
           echo ""
           echo "示例:"
           echo "  $0 root@192.168.1.100"
           echo "  $0 /dev/sdc"
           echo "  $0 local"
           exit 1; }

check_prereq() {
    info "检查前置条件..."

    if [ ! -f "$PERI_BIN" ]; then
        error "未找到 peri 二进制文件: $PERI_BIN"
        exit 1
    fi

    # 检查是否为 ARM ELF
    if ! file "$PERI_BIN" | grep -q "ARM"; then
        error "peri 不是 ARM 二进制文件"
        exit 1
    fi

    info "前置条件检查通过"
}

deploy_ssh() {
    local DEST="$1"
    info "通过 SSH 部署到: $DEST"

    # 测试连接
    if ! ssh -o ConnectTimeout=5 "$DEST" "echo 'SSH OK'" > /dev/null 2>&1; then
        error "无法连接到 $DEST，请检查网络和 SSH 配置"
        exit 1
    fi

    # 检查目标目录是否存在
    if ! ssh "$DEST" "[ -d $TARGET_DIR ]" 2>/dev/null; then
        info "创建目标目录: $TARGET_DIR"
        ssh "$DEST" "sudo mkdir -p $TARGET_DIR $LIB_TARGET_DIR"
    fi

    # 传输文件
    info "传输 peri 二进制..."
    scp "$PERI_BIN" "$DEST:$TARGET_DIR/peri"
    ssh "$DEST" "sudo chmod +x $TARGET_DIR/peri"

    info "传输依赖库..."
    scp -r "$LIB_DIR"/* "$DEST:$LIB_TARGET_DIR/"

    info "设置权限..."
    ssh "$DEST" "sudo chmod +x $LIB_TARGET_DIR/*.so*"

    # 验证
    info "验证部署..."
    ssh "$DEST" "LD_LIBRARY_PATH=$LIB_TARGET_DIR $TARGET_DIR/peri --version" \
        || warn "运行验证失败（可能需要 LLM API key）"

    info "部署完成!"
    info ""
    info "运行方式（需要先设置 API key）:"
    info "  LD_LIBRARY_PATH=$LIB_TARGET_DIR $TARGET_DIR/peri --print '你好'"
    info ""
    info "设置 API key:"
    info "  echo 'ANTHROPIC_API_KEY=sk-...' | ssh $DEST 'sudo tee /etc/peri/env'"
    info "  然后修改 .bashrc 添加: export ANTHROPIC_API_KEY"
}

deploy_sdcard() {
    local DEV="$1"
    info "部署到 SD 卡: $DEV"

    if [ ! -b "$DEV" ]; then
        error "不是有效的块设备: $DEV"
        exit 1
    fi

    # 检查设备大小（避免误操作系统盘）
    DEV_SIZE=$(blockdev --getsize64 "$DEV" 2>/dev/null || echo 0)
    if [ "$DEV_SIZE" -lt 1000000000 ]; then
        warn "设备容量小于 1GB，可能是系统盘，继续需确认"
        read -p "继续? (y/N) " -n 1 -r; echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            info "已取消"
            exit 0
        fi
    fi

    # 挂载点
    MOUNT_POINT=$(mktemp -d)
    trap "umount '$MOUNT_POINT' 2>/dev/null; rmdir '$MOUNT_POINT'" EXIT

    # 尝试挂载
    if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
        info "使用已挂载的: $MOUNT_POINT"
    else
        # 尝试挂载第一个分区
        if [ -b "${DEV}1" ]; then
            mount "${DEV}1" "$MOUNT_POINT" 2>/dev/null || {
                warn "无法挂载 ${DEV}1，尝试直接复制"
                MOUNT_POINT=""
            }
        else
            MOUNT_POINT=""
        fi
    fi

    if [ -n "$MOUNT_POINT" ]; then
        DEST_DIR="$MOUNT_POINT/opt/peri"
        info "复制文件到: $DEST_DIR"
        mkdir -p "$DEST_DIR/lib"
        cp "$PERI_BIN" "$DEST_DIR/peri"
        cp -r "$LIB_DIR"/* "$DEST_DIR/lib/"
        chmod +x "$DEST_DIR/peri" "$DEST_DIR/lib"/*.so*
        info "部署完成! 挂载 SD 卡到开发板后:"
        info "  LD_LIBRARY_PATH=/opt/peri/lib /opt/peri/peri --print '你好'"
    else
        error "需要先挂载 SD 卡分区"
        info "示例: mount ${DEV}1 /mnt/sd && cp -r $SCRIPT_DIR/* /mnt/sd/opt/"
    fi
}

package_local() {
    info "创建本地部��包..."
    PKG_NAME="peri-tlt153-v0.1.0.tar.gz"
    cd "$SCRIPT_DIR"
    tar -czf "$PKG_NAME"peri-$(date +%Y%m%d).tar.gz \
        --exclude='*.sh' \
        --exclude='deploy.sh' \
        peri \
        lib/ \
        2>/dev/null || tar -czf "$PKG_NAME" peri lib/
    info "已打包: $PKG_NAME"
    ls -lh "$SCRIPT_DIR/$PKG_NAME"
}

# 主流程
main() {
    info "peri TLT153-MiniEVM 部署工具"
    info "================================"

    check_prereq

    if [ $# -eq 0 ]; then
        echo ""
        echo "请选择部署方式:"
        echo "  1) 通过 SSH 部署到远程设备"
        echo "  2) 部署到本地 SD 卡"
        echo "  3) 仅打包本地部署包"
        echo "  4) 退出"
        read -p "选择 (1-4): " choice
        case "$choice" in
            1) read -p "目标设备 (user@ip): " DEST; deploy_ssh "$DEST" ;;
            2) read -p "SD 卡设备 (/dev/sdX): " DEV; deploy_sdcard "$DEV" ;;
            3) package_local ;;
            *) info "退出"; exit 0 ;;
        esac
    else
        case "$1" in
            local)  package_local ;;
            root@*) deploy_ssh "$1" ;;
            /dev/*) deploy_sdcard "$1" ;;
            *)      usage ;;
        esac
    fi
}

main "$@"
