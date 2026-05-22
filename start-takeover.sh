#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONITOR_BIN="${SCRIPT_DIR}/target/release/ollama-monitor"

# 颜色
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "========================================"
echo "  Ollama Monitor — Takeover Mode"
echo "========================================"
echo ""

# 检查编译产物
if [ ! -f "$MONITOR_BIN" ]; then
    echo -e "${RED}错误: 未找到 monitor 可执行文件${NC}"
    echo "请先编译: cargo build --release"
    exit 1
fi

# 检查 11434 是否被占用
if lsof -Pi :11434 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${YELLOW}发现进程占用 11434 端口，正在停止...${NC}"
    killall ollama 2>/dev/null || true
    sleep 2
    # 再次检查
    if lsof -Pi :11434 -sTCP:LISTEN -t >/dev/null 2>&1; then
        echo -e "${RED}无法释放 11434 端口，请手动检查: lsof -i :11434${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ 11434 已释放${NC}"
fi

# 检查 11436 是否空闲
if lsof -Pi :11436 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${YELLOW}警告: 11436 已被占用，可能 Ollama 已经在该端口运行${NC}"
else
    # 启动 Ollama 在 11436
    echo -e "${GREEN}启动 Ollama 在 127.0.0.1:11436...${NC}"
    OLLAMA_HOST=127.0.0.1:11436 ollama serve >/dev/null 2>&1 &
    OLLAMA_PID=$!

    # 等待 Ollama 就绪
    for i in {1..10}; do
        if curl -s http://127.0.0.1:11436/api/tags >/dev/null 2>&1; then
            echo -e "${GREEN}✓ Ollama 就绪 (PID: $OLLAMA_PID)${NC}"
            break
        fi
        sleep 1
    done

    # 设置退出时清理
    trap "echo ''; echo '清理 Ollama...'; kill $OLLAMA_PID 2>/dev/null || true" EXIT
fi

echo ""
echo -e "${GREEN}启动 Monitor (接管 11434 → 11436)...${NC}"
echo "按 q 退出"
echo ""

# 启动 monitor
exec "$MONITOR_BIN" --takeover
