# 公開用サンプル匿名化ツール Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `input-samples/` の実写真から難ケース代表6枚前後を厳選し、顔とスライド内部をモザイク化・EXIF除去した公開用サンプルを `samples/`（git追跡）に生成する運用ツールを作る。

**Architecture:** Python 側の独立ツール。純粋関数（幾何・画像・パース・検証比較）を `python/scripts/anonymize_core.py` に集約し単体テスト可能にする。torch/PersonSegmenter と Rust 本体 subprocess を使うオーケストレーションは `python/scripts/anonymize_samples.py`。検証ゲートは Rust 本体（＝検出の正）を subprocess で呼び、元と匿名化版の検出 quad/method/aspect/confidence を機械比較する。

**Tech Stack:** Python 3.12（stdlib `tomllib`）、OpenCV(`opencv-python-headless`)、numpy、torchvision DeepLabV3（`[ml]` extra、遅延import）、pytest（新規 dev 依存）、Rust 本体 `target/release/slidewarp --dump-geom`。

## Global Constraints

- スライドの**四辺エッジにはモザイクをかけない**（検出テスト性の維持が絶対条件）。内側マージン既定 `mosaic_inset = 0.06`。
- モザイクは pixelate（ぼかしでなくブロック化）。既定 `mosaic_block = 16`（領域短辺に対する分割数の目安）。
- 顔は人物マスク上部バンド（既定 `face_band = 0.22`）から生成。
- 出力は EXIF/メタデータを含まない（`cv2.imwrite` は EXIF を書かないため、imdecode→imwrite で構造的に除去される）。
- 座標系は EXIF 回転適用後（`cv2.imdecode(..., IMREAD_COLOR)` と Rust `load_oriented` が一致）。
- 公開物は Rust 本体で自動チェック PASS（quad 各隅ズレ < 画像長辺の 1%、method 一致、aspect 一致、confidence 差 < 0.03）＋目視 OK のものだけ。
- 検出アルゴリズム由来の値は Rust `target/release/slidewarp` を**最新ソースで再ビルド**してから使う（`cargo build --release`）。
- テストは `python/` で `uv run pytest`。純粋関数のみ対象（torch/subprocess/実写真を要する部分は実行と目視で検証）。

---

## File Structure

- Create: `python/scripts/anonymize_core.py` — 純粋関数（幾何/画像/パース/検証比較/review.html生成）。torch を import 時に読み込まない。
- Create: `python/scripts/anonymize_samples.py` — CLI オーケストレーション。`anonymize_core` を import、`slidewarp.ml.PersonSegmenter` を遅延 import。
- Create: `python/samples.toml` — 厳選リストと画像ごとの手動オーバーライド。
- Create: `python/tests/test_anonymize_core.py` — 純粋関数の単体テスト。
- Create: `samples/README.md` — 各公開画像のテスト目的。
- Modify: `python/pyproject.toml` — `dev` extra に pytest 追加、`[tool.pytest.ini_options]`。
- Modify: `.gitignore` — `samples/_staging/` を追記。

---

## Task 1: プロジェクト設定（テスト基盤・依存・ディレクトリ・gitignore）

**Files:**
- Modify: `python/pyproject.toml`
- Modify: `.gitignore`
- Create: `python/tests/test_smoke.py`

**Interfaces:**
- Consumes: なし
- Produces: `uv run pytest` が実行可能。`scripts` ディレクトリが `pythonpath` に載り `import anonymize_core` 可能。

- [ ] **Step 1: `python/pyproject.toml` に dev extra と pytest 設定を追加**

`[project.optional-dependencies]` の `ml = [...]` の直後に追記し、末尾に pytest 設定を足す:

```toml
[project.optional-dependencies]
ml = [
    "torch>=2.2",
    "torchvision>=0.17",
]
dev = [
    "pytest>=8",
]

[tool.pytest.ini_options]
pythonpath = ["scripts"]
testpaths = ["tests"]
```

- [ ] **Step 2: `.gitignore` に staging を追記**

`# 処理出力・生成物` ブロックの `out/` の行の直後に追記:

```
samples/_staging/
```

- [ ] **Step 3: スモークテストを作成**

`python/tests/test_smoke.py`:

```python
def test_smoke():
    assert 1 + 1 == 2
```

- [ ] **Step 4: 依存を同期しテストを実行**

Run: `cd python && uv sync --extra dev && uv run pytest -q`
Expected: `1 passed`

- [ ] **Step 5: コミット**

```bash
git add python/pyproject.toml .gitignore python/tests/test_smoke.py
git commit -m "chore(anonymize): pytest 基盤と dev 依存・staging gitignore を追加"
```

---

## Task 2: 幾何コア — inset_quad

**Files:**
- Create: `python/scripts/anonymize_core.py`
- Test: `python/tests/test_anonymize_core.py`

**Interfaces:**
- Consumes: なし
- Produces: `inset_quad(quad: np.ndarray, inset: float) -> np.ndarray`。quad は (4,2) float。各頂点を重心方向に `inset` 割合だけ寄せた (4,2) を返す（辺を内側に縮める）。

- [ ] **Step 1: 失敗するテストを書く**

`python/tests/test_anonymize_core.py`:

```python
import numpy as np
import anonymize_core as ac


def test_inset_quad_shrinks_toward_centroid():
    quad = np.array([[0, 0], [100, 0], [100, 100], [0, 100]], dtype=np.float32)
    out = ac.inset_quad(quad, 0.1)
    # 重心(50,50)へ各頂点が10%寄る -> 角は(10,10),(90,10),(90,90),(10,90)
    expected = np.array([[10, 10], [90, 10], [90, 90], [10, 90]], dtype=np.float32)
    assert np.allclose(out, expected, atol=1e-4)
    # 元の quad は破壊しない
    assert quad[0, 0] == 0
```

- [ ] **Step 2: 実行して失敗を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py::test_inset_quad_shrinks_toward_centroid -q`
Expected: FAIL（`ModuleNotFoundError: No module named 'anonymize_core'` または `AttributeError`）

- [ ] **Step 3: 最小実装**

`python/scripts/anonymize_core.py`（新規、ファイル冒頭）:

```python
"""公開用サンプル匿名化の純粋関数（幾何・画像・パース・検証・review生成）。

torch は import しない（単体テストを軽量に保つため）。
"""

from __future__ import annotations

from dataclasses import dataclass, field

import cv2
import numpy as np


def inset_quad(quad: np.ndarray, inset: float) -> np.ndarray:
    """quad の各頂点を重心方向に inset 割合だけ寄せた (4,2) を返す。"""
    q = np.asarray(quad, dtype=np.float32)
    c = q.mean(axis=0)
    return (q + (c - q) * float(inset)).astype(np.float32)
```

- [ ] **Step 4: 実行して成功を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py::test_inset_quad_shrinks_toward_centroid -q`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add python/scripts/anonymize_core.py python/tests/test_anonymize_core.py
git commit -m "feat(anonymize): inset_quad を追加"
```

---

## Task 3: 画像コア — pixelate / build_mosaic_mask / apply_mosaic / strip_and_write

**Files:**
- Modify: `python/scripts/anonymize_core.py`
- Test: `python/tests/test_anonymize_core.py`

**Interfaces:**
- Consumes: なし
- Produces:
  - `pixelate(img: np.ndarray, block: int) -> np.ndarray` — 画像を 1/block に縮小し最近傍で元サイズへ拡大したブロック化画像。block>=1。
  - `build_mosaic_mask(shape: tuple[int, int], inner_quad: np.ndarray | None, face_rects: list[tuple[int, int, int, int]]) -> np.ndarray` — (h,w) uint8。モザイク対象=255。inner_quad は fillConvexPoly、face_rects は矩形塗り。
  - `apply_mosaic(img: np.ndarray, mask: np.ndarray, block: int) -> np.ndarray` — mask=255 の画素だけ pixelate(img,block) に差し替えた画像。
  - `strip_and_write(img_bgr: np.ndarray, out_path, quality: int = 95) -> None` — EXIF を持たない画像として書き出す（cv2.imwrite）。

- [ ] **Step 1: 失敗するテストを書く**

`python/tests/test_anonymize_core.py` に追記:

```python
def test_pixelate_blocks_reduce_detail():
    img = np.arange(64 * 64 * 3, dtype=np.uint8).reshape(64, 64, 3)
    out = ac.pixelate(img, block=16)  # 64/16=4x4 に縮小して戻す
    assert out.shape == img.shape
    # 左上 16x16 ブロックは単一色になる（ブロック内が一定）
    block = out[0:16, 0:16].reshape(-1, 3)
    assert np.all(block == block[0])


def test_build_mask_covers_quad_not_edges():
    mask = ac.build_mosaic_mask((100, 100),
                                np.array([[20, 20], [80, 20], [80, 80], [20, 80]], np.float32),
                                [])
    assert mask.shape == (100, 100)
    assert mask[50, 50] == 255      # 内側は対象
    assert mask[0, 0] == 0          # 画像端(辺の外)は非対象


def test_build_mask_adds_face_rects():
    mask = ac.build_mosaic_mask((100, 100), None, [(10, 10, 20, 20)])
    assert mask[15, 15] == 255
    assert mask[50, 50] == 0


def test_apply_mosaic_only_inside_mask():
    img = np.zeros((32, 32, 3), np.uint8)
    img[:] = 100
    img[8:24, 8:24] = np.arange(16 * 16 * 3, dtype=np.uint8).reshape(16, 16, 3)
    mask = np.zeros((32, 32), np.uint8)
    mask[8:24, 8:24] = 255
    out = ac.apply_mosaic(img, mask, block=8)
    assert np.all(out[0, 0] == 100)          # マスク外は不変
    assert not np.array_equal(out[8:24, 8:24], img[8:24, 8:24])  # マスク内は変化
```

- [ ] **Step 2: 実行して失敗を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k "pixelate or mask or mosaic"`
Expected: FAIL（`AttributeError: module 'anonymize_core' has no attribute 'pixelate'`）

- [ ] **Step 3: 最小実装**

`python/scripts/anonymize_core.py` に追記:

```python
def pixelate(img: np.ndarray, block: int) -> np.ndarray:
    """画像を 1/block に縮小し最近傍で元サイズへ戻したブロック化画像を返す。"""
    h, w = img.shape[:2]
    block = max(1, int(block))
    sw, sh = max(1, w // block), max(1, h // block)
    small = cv2.resize(img, (sw, sh), interpolation=cv2.INTER_AREA)
    return cv2.resize(small, (w, h), interpolation=cv2.INTER_NEAREST)


def build_mosaic_mask(shape, inner_quad, face_rects) -> np.ndarray:
    """(h,w) uint8 マスク。モザイク対象=255。"""
    h, w = shape
    mask = np.zeros((h, w), np.uint8)
    if inner_quad is not None:
        cv2.fillConvexPoly(mask, np.asarray(inner_quad, np.int32), 255)
    for (x, y, rw, rh) in face_rects:
        x0, y0 = max(0, x), max(0, y)
        x1, y1 = min(w, x + rw), min(h, y + rh)
        if x1 > x0 and y1 > y0:
            mask[y0:y1, x0:x1] = 255
    return mask


def apply_mosaic(img: np.ndarray, mask: np.ndarray, block: int) -> np.ndarray:
    """mask=255 の画素だけ pixelate 画像に差し替える。"""
    pix = pixelate(img, block)
    out = img.copy()
    sel = mask.astype(bool)
    out[sel] = pix[sel]
    return out


def strip_and_write(img_bgr: np.ndarray, out_path, quality: int = 95) -> None:
    """EXIF を持たない画像として書き出す（cv2.imwrite は EXIF を書かない）。"""
    from pathlib import Path

    p = Path(out_path)
    p.parent.mkdir(parents=True, exist_ok=True)
    params = [cv2.IMWRITE_JPEG_QUALITY, int(quality)] if p.suffix.lower() in (".jpg", ".jpeg") else []
    ok = cv2.imwrite(str(p), img_bgr, params)
    if not ok:
        raise RuntimeError(f"書き出し失敗: {p}")
```

- [ ] **Step 4: 実行して成功を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k "pixelate or mask or mosaic"`
Expected: PASS（4 件）

- [ ] **Step 5: コミット**

```bash
git add python/scripts/anonymize_core.py python/tests/test_anonymize_core.py
git commit -m "feat(anonymize): pixelate/マスク生成/モザイク適用/書き出しを追加"
```

---

## Task 4: 顔バンド生成 — face_bands_from_person_mask

**Files:**
- Modify: `python/scripts/anonymize_core.py`
- Test: `python/tests/test_anonymize_core.py`

**Interfaces:**
- Consumes: なし
- Produces: `face_bands_from_person_mask(person_mask: np.ndarray, band: float, min_area: int = 400) -> list[tuple[int, int, int, int]]` — 人物マスク(255)の連結成分ごとに、面積 >= min_area のものについて bbox 上部 `band` 割合の矩形 (x, y, w, int(h*band)) を返す。

- [ ] **Step 1: 失敗するテストを書く**

`python/tests/test_anonymize_core.py` に追記:

```python
def test_face_bands_top_of_person():
    m = np.zeros((300, 300), np.uint8)
    m[50:250, 100:180] = 255   # 高さ200,幅80 の人物塊（面積>min）
    rects = ac.face_bands_from_person_mask(m, band=0.25, min_area=400)
    assert len(rects) == 1
    x, y, w, h = rects[0]
    assert (x, y, w) == (100, 50, 80)
    assert h == int(200 * 0.25)   # 上部25% = 50px


def test_face_bands_skips_small_blobs():
    m = np.zeros((100, 100), np.uint8)
    m[10:15, 10:15] = 255         # 面積25 < min_area
    assert ac.face_bands_from_person_mask(m, band=0.25, min_area=400) == []
```

- [ ] **Step 2: 実行して失敗を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k face_bands`
Expected: FAIL

- [ ] **Step 3: 最小実装**

`python/scripts/anonymize_core.py` に追記:

```python
def face_bands_from_person_mask(person_mask, band, min_area=400):
    """人物マスクの連結成分ごとに bbox 上部 band 割合を顔矩形として返す。"""
    n, _, stats, _ = cv2.connectedComponentsWithStats((person_mask > 0).astype(np.uint8), 8)
    rects = []
    for i in range(1, n):  # 0 は背景
        x, y, w, h, area = stats[i]
        if area < min_area:
            continue
        rects.append((int(x), int(y), int(w), max(1, int(h * float(band)))))
    return rects
```

- [ ] **Step 4: 実行して成功を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k face_bands`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add python/scripts/anonymize_core.py python/tests/test_anonymize_core.py
git commit -m "feat(anonymize): 人物マスク上部から顔バンド矩形を生成"
```

---

## Task 5: Rust 出力パーサ — parse_dump_geom / parse_confidence

**Files:**
- Modify: `python/scripts/anonymize_core.py`
- Test: `python/tests/test_anonymize_core.py`

**Interfaces:**
- Consumes: なし
- Produces:
  - `@dataclass DumpGeom`: `method: str`, `aspect: str | None`, `quad: np.ndarray | None`（(4,2) float、順序は detect の TL,TR,BR,BL）。
  - `parse_dump_geom(stdout: str) -> dict[str, DumpGeom]` — `--dump-geom` の各行をファイル名（末尾、空白を含みうる）で引ける辞書に。`none  <name>` 行は quad=None。
  - `parse_confidence(stdout: str) -> dict[str, float]` — 通常実行の `[TAG] name  conf=0.87 ...` 行から名前→confidence。

**Note（Rust 出力形式）:** dump-geom 行は
`"{method:5} est=.. rec=.. persp=.. [16:9] quad=(x,y) (x,y) (x,y) (x,y)  <name>"`
（quad の後は**スペース2つ**で name。name は空白を含みうる）。conf 行は
`"[OK  ] <name>  conf=0.87 <method> .."`。

- [ ] **Step 1: 失敗するテストを書く**

`python/tests/test_anonymize_core.py` に追記:

```python
def test_parse_dump_geom_with_spaced_name():
    line = ("hough est=1.330 rec=Some(1.78) persp=Some(0.05) [16:9] "
            "quad=(10,20) (500,25) (505,400) (12,395)  2026-06-23 08.44.43.jpg")
    d = ac.parse_dump_geom(line + "\nnone  weird name.jpg\n")
    g = d["2026-06-23 08.44.43.jpg"]
    assert g.method == "hough"
    assert g.aspect == "16:9"
    assert g.quad.shape == (4, 2)
    assert list(g.quad[0]) == [10, 20]
    assert list(g.quad[2]) == [505, 400]
    assert d["weird name.jpg"].quad is None
    assert d["weird name.jpg"].method == "none"


def test_parse_confidence_with_spaced_name():
    out = "対象 2 枚 / 並列 4\n[OK  ] 2026-06-23 08.44.43.jpg  conf=0.87 hough\n[LOW ] x.jpg  conf=0.20 minrect\n"
    c = ac.parse_confidence(out)
    assert c["2026-06-23 08.44.43.jpg"] == 0.87
    assert c["x.jpg"] == 0.20
```

- [ ] **Step 2: 実行して失敗を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k parse`
Expected: FAIL

- [ ] **Step 3: 最小実装**

`python/scripts/anonymize_core.py` に追記（`import re` を必要とするため、ファイル冒頭の import 群に `import re` を足す）:

```python
@dataclass
class DumpGeom:
    method: str
    aspect: str | None
    quad: np.ndarray | None


def parse_dump_geom(stdout: str) -> dict[str, DumpGeom]:
    result: dict[str, DumpGeom] = {}
    corner_re = re.compile(r"\(-?\d+,-?\d+\)")
    for line in stdout.splitlines():
        line = line.rstrip()
        if not line:
            continue
        if line.startswith("none "):
            name = line[len("none "):].strip()
            result[name] = DumpGeom("none", None, None)
            continue
        m = re.search(r"\[(4:3|16:9)\]\s+quad=(.+?)\s{2,}(.+)$", line)
        if not m:
            continue
        aspect, quad_str, name = m.group(1), m.group(2), m.group(3).strip()
        method = line.split(None, 1)[0]
        pts = [tuple(int(v) for v in c.strip("()").split(",")) for c in corner_re.findall(quad_str)]
        quad = np.array(pts, dtype=np.float32) if len(pts) == 4 else None
        result[name] = DumpGeom(method, aspect, quad)
    return result


def parse_confidence(stdout: str) -> dict[str, float]:
    result: dict[str, float] = {}
    for line in stdout.splitlines():
        m = re.search(r"^\[.{1,6}\]\s+(.+?)\s+conf=([\d.]+)", line)
        if m:
            result[m.group(1).strip()] = float(m.group(2))
    return result
```

- [ ] **Step 4: 実行して成功を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k parse`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add python/scripts/anonymize_core.py python/tests/test_anonymize_core.py
git commit -m "feat(anonymize): Rust dump-geom/conf 出力パーサを追加"
```

---

## Task 6: 検証比較 — compare

**Files:**
- Modify: `python/scripts/anonymize_core.py`
- Test: `python/tests/test_anonymize_core.py`

**Interfaces:**
- Consumes: `DumpGeom`（Task 5）
- Produces:
  - `@dataclass VerifyResult`: `passed: bool`, `reasons: list[str]`, `quad_drift_px: float`, `conf_delta: float`。
  - `compare(orig: DumpGeom, anon: DumpGeom, orig_conf, anon_conf, img_long_side: int, quad_tol_frac: float = 0.01, conf_tol: float = 0.03) -> VerifyResult` — quad 各隅ズレ（index対応）・method 一致・aspect 一致・conf 差を判定。1つでも外れたら passed=False で理由を列挙。

- [ ] **Step 1: 失敗するテストを書く**

`python/tests/test_anonymize_core.py` に追記:

```python
def _dg(method="hough", aspect="16:9", quad=((0, 0), (100, 0), (100, 60), (0, 60))):
    return ac.DumpGeom(method, aspect, None if quad is None else np.array(quad, np.float32))


def test_compare_pass_when_identical():
    r = ac.compare(_dg(), _dg(), 0.80, 0.80, img_long_side=1000)
    assert r.passed
    assert r.reasons == []


def test_compare_fail_on_quad_drift():
    moved = _dg(quad=((0, 0), (100, 0), (100, 60), (0, 90)))  # 隅が30pxずれ
    r = ac.compare(_dg(), moved, 0.80, 0.80, img_long_side=1000)  # tol=10px
    assert not r.passed
    assert any("quad" in x for x in r.reasons)
    assert r.quad_drift_px >= 29


def test_compare_fail_on_method_and_conf():
    r = ac.compare(_dg(method="hough"), _dg(method="contour"), 0.80, 0.70, img_long_side=1000)
    assert not r.passed
    assert any("method" in x for x in r.reasons)
    assert any("conf" in x for x in r.reasons)


def test_compare_fail_on_missing_quad():
    r = ac.compare(_dg(), _dg(quad=None), 0.8, 0.8, img_long_side=1000)
    assert not r.passed
```

- [ ] **Step 2: 実行して失敗を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k compare`
Expected: FAIL

- [ ] **Step 3: 最小実装**

`python/scripts/anonymize_core.py` に追記:

```python
@dataclass
class VerifyResult:
    passed: bool
    reasons: list[str] = field(default_factory=list)
    quad_drift_px: float = 0.0
    conf_delta: float = 0.0


def compare(orig, anon, orig_conf, anon_conf, img_long_side,
            quad_tol_frac=0.01, conf_tol=0.03) -> VerifyResult:
    reasons: list[str] = []
    drift = float("inf")
    if orig.quad is None or anon.quad is None:
        reasons.append("quad 未検出（元または匿名化版で検出できず）")
    else:
        drift = float(np.max(np.linalg.norm(orig.quad - anon.quad, axis=1)))
        if drift >= quad_tol_frac * img_long_side:
            reasons.append(f"quad ズレ {drift:.1f}px >= 許容 {quad_tol_frac * img_long_side:.1f}px")
    if orig.method != anon.method:
        reasons.append(f"method 不一致 {orig.method}->{anon.method}")
    if orig.aspect != anon.aspect:
        reasons.append(f"aspect 不一致 {orig.aspect}->{anon.aspect}")
    conf_delta = abs(float(orig_conf) - float(anon_conf))
    if conf_delta >= conf_tol:
        reasons.append(f"conf 差 {conf_delta:.3f} >= 許容 {conf_tol}")
    return VerifyResult(passed=not reasons, reasons=reasons,
                        quad_drift_px=(0.0 if drift == float('inf') else drift),
                        conf_delta=conf_delta)
```

- [ ] **Step 4: 実行して成功を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k compare`
Expected: PASS

- [ ] **Step 5: コミット**

```bash
git add python/scripts/anonymize_core.py python/tests/test_anonymize_core.py
git commit -m "feat(anonymize): 元/匿名化版の検出回帰を比較する compare を追加"
```

---

## Task 7: レビュー HTML 生成 — write_review_html

**Files:**
- Modify: `python/scripts/anonymize_core.py`
- Test: `python/tests/test_anonymize_core.py`

**Interfaces:**
- Consumes: `VerifyResult`（Task 6）
- Produces: `write_review_html(entries: list[dict], out_path) -> None` — 各 entry は `{"name": str, "orig_rel": str, "anon_rel": str, "verify": VerifyResult}`。元｜匿名化版を横並び表示し、PASS/FAIL と理由・drift・conf差を注記した HTML を書き出す。

- [ ] **Step 1: 失敗するテストを書く**

`python/tests/test_anonymize_core.py` に追記:

```python
def test_write_review_html(tmp_path):
    entries = [{
        "name": "a.jpg",
        "orig_rel": "../../input-samples/a.jpg",
        "anon_rel": "a.jpg",
        "verify": ac.VerifyResult(passed=True, reasons=[], quad_drift_px=1.2, conf_delta=0.01),
    }, {
        "name": "b.jpg",
        "orig_rel": "../../input-samples/b.jpg",
        "anon_rel": "b.jpg",
        "verify": ac.VerifyResult(passed=False, reasons=["quad ズレ 30px"], quad_drift_px=30.0),
    }]
    out = tmp_path / "review.html"
    ac.write_review_html(entries, out)
    html = out.read_text(encoding="utf-8")
    assert "a.jpg" in html and "b.jpg" in html
    assert "PASS" in html and "FAIL" in html
    assert "quad ズレ 30px" in html
    assert "../../input-samples/a.jpg" in html
```

- [ ] **Step 2: 実行して失敗を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k review_html`
Expected: FAIL

- [ ] **Step 3: 最小実装**

`python/scripts/anonymize_core.py` に追記（`import html as _html` をファイル冒頭 import 群に足す）:

```python
def write_review_html(entries, out_path) -> None:
    from pathlib import Path

    rows = []
    for e in entries:
        v = e["verify"]
        badge = "PASS" if v.passed else "FAIL"
        color = "#1a7f37" if v.passed else "#cf222e"
        reasons = "<br>".join(_html.escape(r) for r in v.reasons) or "—"
        rows.append(f"""
        <section class="item">
          <h2>{_html.escape(e['name'])}
            <span class="badge" style="background:{color}">{badge}</span></h2>
          <div class="pair">
            <figure><figcaption>元</figcaption>
              <img src="{_html.escape(e['orig_rel'])}"></figure>
            <figure><figcaption>匿名化版</figcaption>
              <img src="{_html.escape(e['anon_rel'])}"></figure>
          </div>
          <p class="meta">quad drift={v.quad_drift_px:.1f}px / conf差={v.conf_delta:.3f}</p>
          <p class="reasons">{reasons}</p>
        </section>""")
    doc = f"""<!doctype html><html lang="ja"><head><meta charset="utf-8">
<title>anonymize review</title><style>
body{{font-family:sans-serif;margin:1.5rem;background:#fff;color:#111}}
.item{{border:1px solid #ddd;border-radius:8px;padding:1rem;margin:1rem 0}}
.badge{{color:#fff;padding:.1em .6em;border-radius:1em;font-size:.7em;vertical-align:middle}}
.pair{{display:flex;gap:1rem;flex-wrap:wrap}}
figure{{margin:0}} img{{max-width:46vw;height:auto;border:1px solid #ccc}}
.meta{{color:#555;font-size:.9em}} .reasons{{color:#cf222e;white-space:pre-wrap}}
</style></head><body>
<h1>匿名化サンプル レビュー</h1>
<p>各画像で「顔が消えているか / 本文が判読不能か / 辺にモザイクが掛かっていないか」を目視確認してください。</p>
{''.join(rows)}
</body></html>"""
    p = Path(out_path)
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(doc, encoding="utf-8")
```

- [ ] **Step 4: 実行して成功を確認**

Run: `cd python && uv run pytest tests/test_anonymize_core.py -q -k review_html`
Expected: PASS

- [ ] **Step 5: 全テスト実行してコミット**

Run: `cd python && uv run pytest -q`
Expected: 全 PASS

```bash
git add python/scripts/anonymize_core.py python/tests/test_anonymize_core.py
git commit -m "feat(anonymize): レビュー用 review.html 生成を追加"
```

---

## Task 8: CLI オーケストレーション（生成＋検証＋review）と設定

**Files:**
- Create: `python/scripts/anonymize_samples.py`
- Create: `python/samples.toml`

**Interfaces:**
- Consumes: `anonymize_core`（Task 2-7）、`slidewarp.ml.PersonSegmenter`（遅延 import）、Rust `target/release/slidewarp`。
- Produces: CLI。引数なし=生成→staging＋自動チェック＋review.html。`--promote`=自動 PASS 分を `samples/` へコピー。

- [ ] **Step 1: `python/samples.toml` を作成（厳選6枚）**

```toml
# 公開用サンプルの厳選リストと画像ごとの手動オーバーライド。
# quad / faces を書くと自動検出をそれぞれ「置換 / 追加」する。
mosaic_inset = 0.06
mosaic_block = 16
face_band = 0.22

[[image]]
src = "2026-06-23 08.44.43.jpg"
note = "遠景・強い台形歪み・下辺に観客の頭（下辺オクルージョン）"

[[image]]
src = "2026-06-24 19.47.29.jpg"
note = "明壁投影・登壇者が前に立つ"

[[image]]
src = "2026-06-24 19.44.34.jpg"
note = "タイトル直下の区切り線で上端クリップ"

[[image]]
src = "2026-06-23 08.45.45.jpg"
note = "青被りで天井とスライドが地続き"

[[image]]
src = "2026-06-24 19.55.25.jpg"
note = "超斜め・暗所（上辺が唯一の残不良ケース）"

[[image]]
src = "2026-06-25 15.50.38.jpg"
note = "近接ケース"
```

- [ ] **Step 2: `python/scripts/anonymize_samples.py` を作成**

```python
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

    geom = ac.parse_dump_geom(_run([BIN, "--dump-geom", INPUT_DIR]))
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
        short = min(w, h) if inner is None else int(min(
            np.ptp(inner[:, 0]) if inner is not None else w,
            np.ptp(inner[:, 1]) if inner is not None else h))
        block = max(2, short // block_div)
        out = ac.apply_mosaic(img, mask, block)
        ac.strip_and_write(out, STAGING / name)
        done.append(name)
        print(f"[gen] {name} faces={len(faces)} block={block}")
    return done


def verify(names) -> list[dict]:
    """元と staging で Rust を走らせ、名前ごとに VerifyResult を作り entries を返す。"""
    g_orig = ac.parse_dump_geom(_run([BIN, "--dump-geom", INPUT_DIR]))
    g_anon = ac.parse_dump_geom(_run([BIN, "--dump-geom", STAGING]))
    with tempfile.TemporaryDirectory() as t1, tempfile.TemporaryDirectory() as t2:
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
```

- [ ] **Step 3: import と構文の健全性を確認（torch 不要な範囲）**

Run: `cd python && uv run python -c "import sys; sys.path.insert(0,'scripts'); import anonymize_core, importlib.util as u; print('core ok'); import ast; ast.parse(open('scripts/anonymize_samples.py').read()); print('cli parse ok')"`
Expected: `core ok` と `cli parse ok`

- [ ] **Step 4: コミット**

```bash
git add python/scripts/anonymize_samples.py python/samples.toml
git commit -m "feat(anonymize): 生成+検証+review の CLI オーケストレーションと samples.toml を追加"
```

---

## Task 9: エンドツーエンド実行・目視レビュー・昇格・README・公開コミット

**Files:**
- Create: `samples/README.md`
- Create（生成物・昇格）: `samples/*.jpg`

**Interfaces:**
- Consumes: Task 8 の CLI、Rust 本体、`input-samples/` の実写真、torch。
- Produces: `samples/` に匿名化済み公開サンプル＋README。

- [ ] **Step 1: Rust 本体を最新ソースで再ビルド**

Run: `cd /workspaces/.../slidewarp && cargo build --release`
Expected: `Finished`（約2分）。`target/release/slidewarp` が更新される。

- [ ] **Step 2: ML 依存を導入して生成＋検証を実行**

Run: `cd python && uv sync --extra ml --extra dev && uv run python scripts/anonymize_samples.py`
Expected: 各画像で `[gen] ...`、末尾に `検証: N/6 PASS` と各画像の PASS/FAIL、`review.html` のパス。torch モデルは初回自動DL。

- [ ] **Step 3: 目視レビュー（手動チェックポイント）**

`samples/_staging/review.html` をブラウザで開き、各画像について確認:
- (a) 顔が消えているか（観客の融合塊で取りこぼしが無いか）
- (b) スライド本文が判読不能か（著作権）
- (c) スライドの辺にモザイクが掛かっていないか

FAIL または目視 NG の画像は `python/samples.toml` に手動オーバーライドを追記して対処:
- 内部塗り残し/辺かかり → `quad = [[x,y],[x,y],[x,y],[x,y]]`（TL,TR,BR,BL、EXIF回転後座標）で quad を置換。
- 顔取りこぼし → `faces = [[x,y,w,h], ...]` を追加。
追記後に Step 2 を再実行し、対象画像が PASS＋目視 OK になるまで反復。

- [ ] **Step 4: 全 PASS＋目視 OK を確認したら昇格**

Run: `cd python && uv run python scripts/anonymize_samples.py --promote`
Expected: `[promote] <name>` が各画像で出力され、`samples/*.jpg` が作られる。

- [ ] **Step 5: `samples/README.md` を作成**

各画像の「何をテストするか」を `samples.toml` の note を元に記載する。実際に昇格された画像だけを列挙すること:

```markdown
# samples

slidewarp の検出頑健性を検証するための**公開用サンプル**です。元写真（`input-samples/`、非追跡）から
難ケース代表を厳選し、プライバシー保護（顔のモザイク）・著作権配慮（スライド内部のモザイク）・
EXIF 除去を施しています。スライドの**四辺エッジは無処理**で、検出アルゴリズムの回帰テスト性を保っています。

| ファイル | テストする難ケース |
|---|---|
| 2026-06-23 08.44.43.jpg | 遠景・強い台形歪み・下辺に観客の頭（下辺オクルージョン） |
| 2026-06-24 19.47.29.jpg | 明壁投影・登壇者が前に立つ |
| 2026-06-24 19.44.34.jpg | タイトル直下の区切り線で上端クリップ |
| 2026-06-23 08.45.45.jpg | 青被りで天井とスライドが地続き |
| 2026-06-24 19.55.25.jpg | 超斜め・暗所（上辺が唯一の残不良ケース） |
| 2026-06-25 15.50.38.jpg | 近接ケース |

生成方法は `python/scripts/anonymize_samples.py`（設計: `docs/superpowers/specs/2026-08-07-anonymize-samples-design.md`）。
```

- [ ] **Step 6: 追跡対象を確認してコミット**

Run: `git status --short samples/`
Expected: `samples/README.md` と `samples/*.jpg` が追跡候補、`samples/_staging/` は出てこない（gitignore 済み）。

```bash
git add samples/README.md samples/*.jpg
git commit -m "feat(samples): 匿名化済み公開サンプルを追加"
```

---

## Self-Review 結果（計画作成者による確認）

- **Spec coverage:** ディレクトリ/追跡→Task1・9、内部モザイク（辺無処理）→Task2,3・8、顔モザイク→Task4・8、EXIF除去→Task3、自動チェック→Task5,6・8、review.html目視→Task7・9、昇格フロー→Task8,9、手動オーバーライド→Task8(設定)・9(運用)、厳選6枚→Task8。全項目に対応タスクあり。
- **Placeholder scan:** TBD/TODO・曖昧指示なし。全コードステップに実コードあり。
- **Type consistency:** `DumpGeom`/`VerifyResult` は Task5,6 で定義し Task7,8 で同名利用。`inset_quad`/`build_mosaic_mask`/`apply_mosaic`/`face_bands_from_person_mask`/`parse_dump_geom`/`parse_confidence`/`compare`/`write_review_html`/`strip_and_write` の名称は定義タスクと利用タスクで一致。
- **既知リスク（設計書参照）:** 顔取りこぼしと著作権塗り残しは Task9 の目視＋手動オーバーライドで担保（機械のみでは保証しない）。
