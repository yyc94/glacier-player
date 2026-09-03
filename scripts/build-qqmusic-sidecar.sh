#!/usr/bin/env bash
set -euo pipefail

QQMUSIC_API_REPOSITORY="https://github.com/L-1124/QQMusicApi.git"
QQMUSIC_API_REVISION="108617ffe80abefec6358717b9f4d3677550db10"
PYINSTALLER_VERSION="6.16.0"
PYINSTALLER_HOOKS_VERSION="2026.7"

project_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
build_root="${GLACIER_SIDECAR_BUILD_DIR:-${project_root}/target/qqmusic-sidecar-build}"
source_dir="${QQMUSIC_API_SOURCE_DIR:-${build_root}/source}"
output="${1:-${project_root}/target/release/glacier-qqmusic-api}"
export UV_CACHE_DIR="${UV_CACHE_DIR:-${build_root}/uv-cache}"

if ! command -v uv >/dev/null 2>&1; then
    echo "error: uv is required to build glacier-qqmusic-api" >&2
    exit 1
fi

mkdir -p "$build_root" "$(dirname "$output")"

if [[ -z "${QQMUSIC_API_SOURCE_DIR:-}" ]]; then
    if [[ ! -d "$source_dir/.git" ]]; then
        git clone --filter=blob:none "$QQMUSIC_API_REPOSITORY" "$source_dir"
    fi
    git -C "$source_dir" fetch --depth 1 origin "$QQMUSIC_API_REVISION"
    git -C "$source_dir" checkout --detach "$QQMUSIC_API_REVISION"
fi

actual_revision=$(git -C "$source_dir" rev-parse HEAD)
if [[ "$actual_revision" != "$QQMUSIC_API_REVISION" ]]; then
    echo "error: QQMusicApi source is $actual_revision, expected $QQMUSIC_API_REVISION" >&2
    exit 1
fi

uv sync --project "$source_dir" --group web --no-dev --frozen
uv pip install \
    --python "$source_dir/.venv/bin/python" \
    "pyinstaller==${PYINSTALLER_VERSION}" \
    "pyinstaller-hooks-contrib==${PYINSTALLER_HOOKS_VERSION}"

dist_dir="$build_root/dist"
work_dir="$build_root/pyinstaller"
rm -rf "$dist_dir" "$work_dir"

(
    cd "$source_dir"
    "$source_dir/.venv/bin/pyinstaller" \
        --noconfirm \
        --clean \
        --onefile \
        --name glacier-qqmusic-api \
        --distpath "$dist_dir" \
        --workpath "$work_dir" \
        --specpath "$build_root" \
        --hidden-import web.src.app \
        --collect-all qqmusic_api \
        --collect-submodules web.src \
        web/run.py
)

install -Dm0755 "$dist_dir/glacier-qqmusic-api" "$output"
echo "Built $output from QQMusicApi $QQMUSIC_API_REVISION"
