#!/bin/bash
# .claude/hooks/safety-bar.sh
#
# PreToolUse hook：在 Bash 工具实际执行前拦截危险命令。
# 退出码 = 2 时阻止命令继续执行，退出码 = 0 时放行。
#
# 输入：JSON 从 stdin 来；用 jq 取 .tool_input.command 字段。
# 用户在主窗口手动确认放行的"破坏性"操作请直接回退本脚本（编辑此文件取消屏蔽某条规则），不要绕过。

set -euo pipefail

INPUT=$(cat)

# 提取要执行的 bash 命令
CMD=$(echo "$INPUT" | jq -r '.tool_input.command // ""')

if [ -z "$CMD" ]; then
  exit 0
fi

# 拦截规则集合（按需追加；每条都附原因）
BLOCK_PATTERNS=(
  # git 危险操作
  'git\s+push\s+(--force|--mirror).*\b(main|master|production)\b'
  'git\s+push\s+.*\b(main|master|production)\b\s+--force'
  'git\s+reset\s+--hard\s+(origin/)?(main|master|production)'

  # 系统层破坏
  'rm\s+-rf\s+/[^[:space:]]*$'
  'rm\s+-rf\s+~'
  'rm\s+-rf\s+\$HOME'
  'sudo\s+rm\s+'
  'sudo\s+(dd|mkfs|fdisk)\s+'
  'dd\s+if=.*\s+of=/dev/'
  'mkfs\.[a-z0-9]+\s+/dev/'
  'chmod\s+777\s+'

  # 云 / 容器危险操作
  'kubectl\s+.*delete.*\bprod\b'
  'kubectl\s+.*apply.*\bprod\b'
  'terraform\s+apply.*'
  'terraform\s+destroy'
  'aws\s+.*--profile\s+prod'

  # 直接发布到生产 npm/pypi 等
  'npm\s+publish'
  'cargo\s+publish'

  # 卸载关键依赖
  'rm\s+-rf\s+(node_modules|target|dist|build)/?\s*$'  # 这条比较 aggressive，列出来但放行
)

# 拦截规则只针对真正高危的命令；像 rm -rf /tmp/* 这种开发常用的不拦
HARD_BLOCKS=(
  # 强推 main/master/production 分支
  'git\s+push\s+(--force|--mirror|-f).*\b(main|master|production)\b'
  'git\s+push\s+.*\b(main|master|production)\b\s+(--force|-f)'
  'git\s+reset\s+--hard\s+(origin/)?(main|master|production)\b'

  # 系统根目录 / 用户主目录的递归删
  # POSIX ERE 不支持 (?!...) 负向先行；改用枚举：拦关键系统目录，/tmp /var/tmp /var/folders 不在列表里所以放行
  'rm\s+-rf\s+/(etc|usr|bin|sbin|home|root|opt|boot|Library|Applications|System)([/[:space:]]|$)'
  'rm\s+-rf\s+/$'
  'rm\s+-rf\s+~/?\s*$'
  'rm\s+-rf\s+\$HOME(/?\s*$|\s+)'
  'rm\s+-rf\s+/Users/[^/[:space:]]+/?\s*$'

  # sudo + 破坏性命令
  'sudo\s+rm\s+-rf\s+/'
  'sudo\s+(dd|mkfs|fdisk)\s+'

  # 设备/磁盘层操作
  'dd\s+if=.*\s+of=/dev/'
  'mkfs\.[a-z0-9]+\s+/dev/'

  # 全开权限到根
  'chmod\s+-R?\s*777\s+/'

  # K8s / Terraform 高危
  'kubectl\s+.*delete.*\bprod\b'
  'terraform\s+destroy\b'

  # 真发包 (避免误发到公共仓库)
  'npm\s+publish\b'
  'cargo\s+publish\b'

  # v5 (ADR-002 / HANDOFF v5 第 636 行)：应用商店上传 — 个人工具不上架
  'xcrun\s+altool\s+.*--upload-app'
  'xcrun\s+notarytool\s+submit'
  'fastlane\s+(release|deliver|pilot)\b'

  # v5 (ADR-002)：删除签名/证书/私钥文件 — 不可逆且后果严重
  'rm\s+(-[a-zA-Z]+\s+)*[^|;&]*\.(jks|p8|p12|pem|key|cer|crt|keystore)\b'
  'security\s+delete-keychain'
  'security\s+import\s+.*-k\s+/Library/Keychains'
)

for pat in "${HARD_BLOCKS[@]}"; do
  if echo "$CMD" | grep -qE "$pat"; then
    echo "🛑「安全栏」拦截高危命令" >&2
    echo "  命令：$CMD" >&2
    echo "  规则：$pat" >&2
    echo "  如果你确实需要执行，请编辑 .claude/hooks/safety-bar.sh 临时移除该规则，或在终端外手动执行。" >&2
    exit 2
  fi
done

# 通过
exit 0
