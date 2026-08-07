"""公開用サンプル匿名化ツール（生成＋Rust本体での検証＋review.html＋昇格）。

使い方（python/ 内で。事前に `cargo build --release`）:
  uv sync --extra ml --extra dev
  uv run python scripts/anonymize_samples.py            # 生成→staging + 検証 + review.html
  uv run python scripts/anonymize_samples.py --promote  # 自動PASS分を samples/ へ昇格
"""

from __future__ import annotations

import argparse
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

import cv2
import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))  # anonymize_core を import 可能に
import anonymize_core as ac

REPO = Path(__file__).resolve().parents[2]           # .../slidewarp
PY = REPO / "python"
INPUT_DIR = REPO / "input-samples"
SAMPLES = REPO / "samples"
STAGING = SAMPLES / "_staging"
BIN = REPO / "target" / "release" / "slidewarp"


def _imread(path: Path) -> np.ndarray:
    data = np.frombuffer(path.read_bytes(), np.uint8)
    img = cv2.imdecode(data, cv2.IMREAD_COLOR)  # EXIF 回転適用（Rust と一致）
    if img is None:
        raise RuntimeError(f"読み込み失敗: {path}")
    return img


def _run(args: list[str]) -> str:
    return subprocess.run([str(a) for a in args], cwd=REPO,
                          capture_output=True, text=True, check=True).stdout


def _load_config():
    with open(PY / "samples.toml", "rb") as f:
        return tomllib.load(f)


def generate(cfg) -> list[str]:
    """厳選画像を匿名化して staging に書き出し、処理したファイル名を返す。"""
    from slidewarp.ml import PersonSegmenter  # 遅延 import（torch）

    if not BIN.exists():
        sys.exit(f"Rust バイナリが無い: {BIN}\n先に `cargo build --release` を実行してください。")
    STAGING.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as _t:
        geom = ac.parse_dump_geom(_run([BIN, "--dump-geom", "-o", _t, INPUT_DIR]))
    seg = PersonSegmenter()
    inset = float(cfg.get("mosaic_inset", 0.06))
    block_div = int(cfg.get("mosaic_block", 16))
    band = float(cfg.get("face_band", 0.22))

    done = []
    for item in cfg["image"]:
        name = item["src"]
        src = INPUT_DIR / name
        if not src.exists():
            print(f"[skip] 入力が無い: {name}")
            continue
        img = _imread(src)
        h, w = img.shape[:2]

        # slide quad: 手動 override 優先、無ければ Rust 検出
        if "quad" in item:
            quad = np.array(item["quad"], np.float32)
        else:
            g = geom.get(name)
            quad = g.quad if g else None
        inner = ac.inset_quad(quad, inset) if quad is not None else None
        if inner is None:
            print(f"[warn] quad 未取得（内部モザイク無し）: {name}")

        # 顔: 人物マスク上部バンド + 手動 override（追加）
        faces = ac.face_bands_from_person_mask(seg.mask(img), band=band)
        faces += [tuple(int(v) for v in r) for r in item.get("faces", [])]

        mask = ac.build_mosaic_mask((h, w), inner, faces)
        short = (min(w, h) if inner is None
                 else int(min(np.ptp(inner[:, 0]), np.ptp(inner[:, 1]))))
        block = max(2, short // block_div)
        out = ac.apply_mosaic(img, mask, block)
        ac.strip_and_write(out, STAGING / name)
        done.append(name)
        print(f"[gen] {name} faces={len(faces)} block={block}")
    return done


def verify(names) -> list[dict]:
    """元と staging で Rust を走らせ、名前ごとに VerifyResult を作り entries を返す。"""
    with tempfile.TemporaryDirectory() as tg, \
         tempfile.TemporaryDirectory() as t1, tempfile.TemporaryDirectory() as t2:
        g_orig = ac.parse_dump_geom(_run([BIN, "--dump-geom", "-o", tg, INPUT_DIR]))
        g_anon = ac.parse_dump_geom(_run([BIN, "--dump-geom", "-o", tg, STAGING]))
        c_orig = ac.parse_confidence(_run([BIN, INPUT_DIR, "-o", t1, "--on-low-confidence", "copy"]))
        c_anon = ac.parse_confidence(_run([BIN, STAGING, "-o", t2, "--on-low-confidence", "copy"]))
    entries = []
    for name in names:
        img = _imread(INPUT_DIR / name)
        long_side = max(img.shape[:2])
        res = ac.compare(g_orig.get(name, ac.DumpGeom("none", None, None)),
                         g_anon.get(name, ac.DumpGeom("none", None, None)),
                         c_orig.get(name, 0.0), c_anon.get(name, 0.0), long_side)
        entries.append({"name": name, "orig_rel": f"../../input-samples/{name}",
                        "anon_rel": name, "verify": res})
    return entries


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--promote", action="store_true",
                    help="自動チェック PASS の画像を samples/ へ昇格コピー")
    args = ap.parse_args()
    cfg = _load_config()

    names = generate(cfg)
    entries = verify(names)
    ac.write_review_html(entries, STAGING / "review.html")

    n_pass = sum(1 for e in entries if e["verify"].passed)
    print(f"\n検証: {n_pass}/{len(entries)} PASS")
    for e in entries:
        v = e["verify"]
        print(f"  [{'PASS' if v.passed else 'FAIL'}] {e['name']}"
              + ("" if v.passed else "  <- " + "; ".join(v.reasons)))
    print(f"\nreview.html: {STAGING / 'review.html'}")

    if args.promote:
        SAMPLES.mkdir(parents=True, exist_ok=True)
        for e in entries:
            if e["verify"].passed:
                dst = SAMPLES / e["name"]
                dst.write_bytes((STAGING / e["name"]).read_bytes())
                print(f"[promote] {e['name']}")
    else:
        print("\n目視で review.html を確認後、--promote で samples/ へ昇格してください。")


if __name__ == "__main__":
    main()
