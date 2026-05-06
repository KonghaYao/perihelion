#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$(dirname "$0")/static/vendor"
cd "$(dirname "$0")/static/vendor"

# Download vendor JS/CSS from unpkg.com into current directory.
# Run this script once to vendor all frontend dependencies.

download() {
    local file="$1"
    local url="$2"
    echo "Downloading $file ..."
    curl -sfL --retry 3 -o "$file" "$url"
    echo "  OK  $(wc -c < "$file" | tr -d ' ') bytes"
}

download lucide.js          https://unpkg.com/lucide@latest/dist/umd/lucide.js
download drawflow.min.css   https://unpkg.com/drawflow@0.0.59/dist/drawflow.min.css
download drawflow.min.js    https://unpkg.com/drawflow@0.0.59/dist/drawflow.min.js
download js-yaml.min.js     https://unpkg.com/js-yaml@4.1.0/dist/js-yaml.min.js
download dagre.min.js       https://unpkg.com/dagre@0.8.5/dist/dagre.min.js

echo ""
echo "All vendor files downloaded to $(pwd)"
