#!/bin/bash

# set -e: Exit immediately if a command exits with a non-zero status.
set -e

# --- Validation ---
if [ "$#" -ne 2 ]; then
    echo "❌ 错误: 参数不正确。"
    echo "用法: $0 <文件后缀> \"<应用程序路径>\""
    echo "示例: $0 txt \"/System/Applications/TextEdit.app\""
    echo "示例: $0 md \"/Applications/Visual Studio Code.app\""
    exit 1
fi

FILE_EXTENSION="$1"
APP_PATH="$2"

if [ ! -d "$APP_PATH" ]; then
    echo "❌ 错误: 找不到指定的应用程序路径: '$APP_PATH'"
    exit 1
fi

# --- Main Logic ---

echo "🚀 开始设置 .${FILE_EXTENSION} 文件的默认打开方式..."

# 1. 获取应用程序的 Bundle Identifier
# duti 需要的是应用的唯一ID, 而不是路径. e.g., com.microsoft.VSCode
echo "🔎 正在查找 '$APP_PATH' 的 Bundle ID..."
BUNDLE_ID=$(mdls -name kMDItemCFBundleIdentifier -r "$APP_PATH")
if [ -z "$BUNDLE_ID" ]; then
    echo "❌ 错误: 无法获取应用程序的 Bundle ID。请检查路径是否为有效的 .app 程序。"
    exit 1
fi
echo "✅ Bundle ID: ${BUNDLE_ID}"

# 2. 获取文件后缀对应的 UTI (Uniform Type Identifier)
# Launch Services 是通过 UTI 来识别文件类型的. e.g., public.plain-text
echo "🔎 正在查找 .${FILE_EXTENSION} 的 UTI..."

# 创建一个临时文件来获取 UTI
TEMP_FILE="temp_file_for_uti.${FILE_EXTENSION}"
touch "$TEMP_FILE"

# 重试循环：最多重试 10 次，每次间隔 1 秒
MAX_RETRIES=10
RETRY_COUNT=0
UTI=""

while [ $RETRY_COUNT -lt $MAX_RETRIES ]; do
    UTI=$(mdls -name kMDItemContentType -r "$TEMP_FILE" 2>/dev/null)
    
    if [ -n "$UTI" ] && [ "$UTI" != "(null)" ]; then
        echo "✅ UTI: ${UTI}"
        break
    else
        RETRY_COUNT=$((RETRY_COUNT + 1))
        if [ $RETRY_COUNT -lt $MAX_RETRIES ]; then
            echo "⏳ 第 ${RETRY_COUNT} 次尝试获取 UTI 失败，等待 1 秒后重试..."
            sleep 1
        else
            echo "❌ 经过 ${MAX_RETRIES} 次尝试，仍无法获取有效的 UTI。"
            echo "❌ 错误: 无法获取 .${FILE_EXTENSION} 的 UTI。这可能是一个未知的后缀。"
            # 清理临时文件
            rm -f "$TEMP_FILE"
            exit 1
        fi
    fi
done

# 清理临时文件
rm -f "$TEMP_FILE"

# 3. 使用 duti 设置默认应用
# duti -s <bundle_id> <uti> <role>
# role 'all' 表示所有角色 (打开, 编辑, 查看等)
echo "⚙️ 正在将 '${BUNDLE_ID}' 设置为 '${UTI}' 的默认处理器..."
duti -s "$BUNDLE_ID" "$UTI" all

echo "✅ 完成! .${FILE_EXTENSION} 文件现在默认会由 $(basename "$APP_PATH") 打开。"
echo "注意: 某些情况下，您可能需要重启 Finder 或注销后重新登录才能看到图标变化。"