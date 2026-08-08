# ダークテーマ sub-slide 対策＋評価基盤信頼化 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ①`--report` を402枚規模でも全数表示できる自己完結HTMLにして信頼できる評価ベースラインを確立し、②ダークテーマ・スライドで内部要素に誤ロックする sub-slide 誤検出を、投影領域マスクによるデュアル候補生成＋ダーク時限定のスコア補正で解消する。

**Architecture:** Rust 本体（`src/report.rs` と `src/detect.rs`）のみを変更する。評価層（report）と検出層（detect）は独立。検出変更は全て「ダークテーマ判定が真のときだけ」効く条件発火とし、明るいスライドの経路はビット単位で不変に保つ（回帰ゼロを構造的に担保）。

**Tech Stack:** Rust 2021 / `image` 0.25 / `imageproc` 0.25 / 既存の純Rust構成（OpenCV非依存・単一バイナリ）。新規依存は追加しない（base64・URLエンコードは自前実装）。

## Global Constraints

- 認識アルゴリズムは **Rust 本体が正**。Python は実験扱いで今回は変更しない。
- **純Rust・OpenCV 非依存・単一バイナリ**を維持。新規クレート依存は追加しない。
- `cargo build --release` は LTO 有効で約2分。**反復は debug（`cargo build` / `cargo test`）**で行い、最終確認だけ release。
- Rust の型推論落とし穴: `x as f64 < y` はコンパイルエラー。必ず **`(x as f64) < y`** と括弧を付ける。
- imageproc の関数は `imageproc::region_labelling::connected_components` のように **フルパス**で呼ぶ既存流儀に合わせる。
- 検出に影響する変更後は **必ず `eval-output/` を削除して作り直し**、全サンプルで**回帰ゼロ**を確認してから採用する（CLAUDE.md 方針）。
- report の変更は**表示層のみ**。検出結果・出力画像・アスペクトには一切影響させない。
- テストフレームワークは未導入だが Rust 標準の `#[cfg(test)]` / `cargo test` を用いる（新規依存なし）。純関数・ヘルパはユニットテスト、画質はサンプル評価で検証する。

---

## ファイル構成

- `src/report.rs`（変更）: サムネイル data URI 埋め込み・URLエンコード・テンプレJS修正。自前 base64/URLエンコードのユニットテストを同ファイル `#[cfg(test)]` に置く。
- `src/main.rs`（変更）: `report::Item` へ絶対パス（サムネ生成用）を渡す。
- `src/detect.rs`（変更）: `screen_mask` / `largest_component_bbox` / `is_dark_theme` の追加、`hough_candidates` の bbox 明示引数化、`detect_slide` のデュアル候補生成、`score_quad`/`edge_profile` の theme-aware 化。ヘルパのユニットテストを同ファイル `#[cfg(test)]` に置く。

---

## Pillar ①: 評価基盤の信頼化（先に完了させる）

### Task 1: 自前 base64 / URL エンコードのユニットテストと実装

**Files:**
- Modify: `src/report.rs`（末尾へ関数追加＋`#[cfg(test)]` モジュール追加）

**Interfaces:**
- Produces:
  - `fn base64_encode(bytes: &[u8]) -> String` — 標準 base64（`+/`、`=` パディング）。
  - `fn url_encode_path(s: &str) -> String` — パス用パーセントエンコード。`/` `.` `-` `_` `~` 英数字はそのまま、それ以外（スペース含む）を `%XX` 化。

- [ ] **Step 1: 失敗するテストを書く**

`src/report.rs` の末尾に追記:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn url_encode_keeps_slash_encodes_space() {
        assert_eq!(url_encode_path("2026-08-04 09.15.43.jpg"), "2026-08-04%2009.15.43.jpg");
        assert_eq!(url_encode_path("../a b/c.jpg"), "../a%20b/c.jpg");
        assert_eq!(url_encode_path("plain_file-1.0~x.jpg"), "plain_file-1.0~x.jpg");
    }
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --bin slidewarp report::tests 2>&1 | tail -20`（バイナリクレートのため `cargo test` で可）
Expected: コンパイルエラー `cannot find function base64_encode` 等で FAIL。

- [ ] **Step 3: 最小実装を書く**

`#[cfg(test)]` モジュールの直前（`TEMPLATE` 定数の後ろ）に追加:

```rust
fn base64_encode(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { T[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn url_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let keep = b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_' | b'~');
        if keep {
            out.push(b as char);
        } else {
            out.push('%');
            out.push_str(&format!("{:02X}", b));
        }
    }
    out
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --bin slidewarp report::tests 2>&1 | tail -20`
Expected: `test result: ok. 2 passed`。

- [ ] **Step 5: コミット**

```bash
git add src/report.rs
git commit -m "feat(report): 自前 base64/URLエンコードとユニットテストを追加"
```

---

### Task 2: サムネイル data URI 生成ヘルパ

**Files:**
- Modify: `src/report.rs`

**Interfaces:**
- Consumes: `base64_encode`（Task 1）。
- Produces: `fn thumb_data_uri(abs: &std::path::Path, max_side: u32) -> Option<String>` — 画像を読み、長辺 `max_side` へ縮小し JPEG(品質70) にエンコードして `data:image/jpeg;base64,...` を返す。読み込み失敗時 `None`。

- [ ] **Step 1: 失敗するテストを書く**

`#[cfg(test)] mod tests` に追記:

```rust
#[test]
fn thumb_data_uri_from_temp_png() {
    use image::{RgbImage, Rgb};
    let mut img = RgbImage::new(1200, 900);
    for p in img.pixels_mut() { *p = Rgb([10, 20, 200]); }
    let dir = std::env::temp_dir();
    let path = dir.join("slidewarp_thumb_test.png");
    img.save(&path).unwrap();
    let uri = thumb_data_uri(&path, 480).expect("some uri");
    assert!(uri.starts_with("data:image/jpeg;base64,"));
    assert!(uri.len() > 200, "uri too short: {}", uri.len());
    assert!(thumb_data_uri(std::path::Path::new("/no/such/file.png"), 480).is_none());
    let _ = std::fs::remove_file(&path);
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --bin slidewarp report::tests::thumb_data_uri_from_temp_png 2>&1 | tail -20`
Expected: `cannot find function thumb_data_uri` で FAIL。

- [ ] **Step 3: 最小実装を書く**

`url_encode_path` の後ろに追加（ファイル冒頭の `use` に `use std::io::Cursor;` を加える）:

```rust
fn thumb_data_uri(abs: &std::path::Path, max_side: u32) -> Option<String> {
    let img = image::open(abs).ok()?;
    let thumb = img.resize(max_side, max_side, image::imageops::FilterType::Triangle);
    let rgb = thumb.to_rgb8();
    let mut buf: Vec<u8> = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(Cursor::new(&mut buf), 70);
    enc.encode_image(&rgb).ok()?;
    Some(format!("data:image/jpeg;base64,{}", base64_encode(&buf)))
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --bin slidewarp report::tests::thumb_data_uri_from_temp_png 2>&1 | tail -20`
Expected: `test result: ok. 1 passed`。

- [ ] **Step 5: コミット**

```bash
git add src/report.rs
git commit -m "feat(report): サムネイル data URI 生成ヘルパを追加"
```

---

### Task 3: report を data URI サムネ＋URLエンコード拡大に切替

**Files:**
- Modify: `src/report.rs:10-51`（`Item` 構造体・`write_report`・シリアライズ）とテンプレJS `src/report.rs:158`,`:168`,`:177`
- Modify: `src/main.rs:300-320`（`Item` 生成箇所）

**Interfaces:**
- Consumes: `thumb_data_uri`, `url_encode_path`（Task 1,2）。
- Produces: `report::Item` に絶対パス2フィールドを追加。シリアライズ時に `src`(=拡大用URLエンコード済み相対), `out`, `src_thumb`(data URI), `out_thumb`(data URI option) を出す。

- [ ] **Step 1: `Item` に絶対パスフィールドを追加**

`src/report.rs` の `pub struct Item`（10-21行）を次に置換:

```rust
#[derive(Clone)]
pub struct Item {
    pub id: usize,
    pub name: String,
    pub src: String,             // 拡大用: out_dir からの相対パス（URLエンコード前）
    pub out: Option<String>,     // 同上（処理後）
    pub src_abs: std::path::PathBuf, // サムネ生成元（絶対）
    pub out_abs: Option<std::path::PathBuf>,
    pub status: String,
    pub confidence: f64,
    pub method: String,
    pub message: String,
    pub parts: serde_json::Value,
}
```

（`#[derive(Serialize)]` は外し、下記の `SerItem` を別途シリアライズする。）

- [ ] **Step 2: `write_report` でサムネ生成しシリアライズ用構造体へ詰める**

`src/report.rs` の `write_report`（30-51行）を次に置換:

```rust
#[derive(Serialize)]
struct SerItem {
    id: usize,
    name: String,
    src: String,
    out: Option<String>,
    src_thumb: Option<String>,
    out_thumb: Option<String>,
    status: String,
    confidence: f64,
    method: String,
    message: String,
    parts: serde_json::Value,
}

#[derive(Serialize)]
struct SerData {
    items: Vec<SerItem>,
    project: String,
    gen: String,
}

pub fn write_report(items: Vec<Item>, out_dir: &Path) -> std::io::Result<std::path::PathBuf> {
    let mut hasher = DefaultHasher::new();
    for it in &items {
        format!("{}:{}:{}:{}", it.name, it.confidence, it.method, it.status).hash(&mut hasher);
    }
    let gen = format!("{:012x}", hasher.finish() & 0xffff_ffff_ffff);
    let ser: Vec<SerItem> = items
        .into_iter()
        .map(|it| SerItem {
            id: it.id,
            name: it.name,
            src: url_encode_path(&it.src),
            out: it.out.as_ref().map(|s| url_encode_path(s)),
            src_thumb: thumb_data_uri(&it.src_abs, 480),
            out_thumb: it.out_abs.as_ref().and_then(|p| thumb_data_uri(p, 480)),
            status: it.status,
            confidence: it.confidence,
            method: it.method,
            message: it.message,
            parts: it.parts,
        })
        .collect();
    let data = SerData {
        items: ser,
        project: out_dir.canonicalize().unwrap_or_else(|_| out_dir.to_path_buf()).display().to_string(),
        gen,
    };
    let payload = serde_json::to_string(&data)
        .unwrap()
        .replace("</", "<\\/")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    let html = TEMPLATE.replace("/*__DATA__*/", &payload);
    std::fs::create_dir_all(out_dir)?;
    let path = out_dir.join("report.html");
    std::fs::write(&path, html)?;
    Ok(path)
}
```

（旧 `#[derive(Serialize)] struct Data {...}` は削除する。）

- [ ] **Step 3: テンプレJS を「表示＝サムネ / 拡大＝フル」に修正**

`src/report.rs:158` の `outImg` 行を次に置換（表示は `out_thumb`、拡大 data 属性に `out`）:

```javascript
  const outImg=it.out_thumb?`<div class="imgwrap"><img loading="lazy" src="${it.out_thumb}" data-full="${it.out||""}" alt=""></div>`:`<p class="meta">出力なし（${it.status}）</p>`;
```

`src/report.rs:168` の元画像行を次に置換:

```javascript
      <div class="imgcol"><h3>元画像</h3><div class="imgwrap"><img loading="lazy" src="${it.src_thumb||""}" data-full="${it.src}" alt=""></div></div>
```

`src/report.rs:177` のズーム設定行を次に置換（拡大時は data-full を優先）:

```javascript
  card.querySelectorAll(".imgwrap img").forEach(img=>img.addEventListener("click",()=>{const z=document.getElementById("zoom");document.getElementById("zoomimg").src=img.dataset.full||img.src;z.showModal();}));
```

- [ ] **Step 4: `main.rs` の Item 生成を新フィールドに対応**

`src/main.rs:309-318` の `report::Item { ... }` を次に置換:

```rust
                report::Item {
                    id: i,
                    name: r.src.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string(),
                    src: rel_path(&args.out_dir, &r.src),
                    out: r.out_path.as_ref().map(|p| rel_path(&args.out_dir, p)),
                    src_abs: r.src.clone(),
                    out_abs: r.out_path.clone(),
                    status: r.status.to_string(),
                    confidence: (r.confidence * 1000.0).round() / 1000.0,
                    method: r.method.to_string(),
                    message: r.message.clone(),
                    parts: r.parts.clone(),
                }
```

（`id: i` と各フィールド名は既存の生成ループの変数に合わせる。既存が `id`/`confidence` 等をどう埋めているかを `src/main.rs:300-320` で確認し、追加した `src_abs`/`out_abs` 以外は既存値を保持すること。）

- [ ] **Step 5: ビルドと小規模生成で全画像表示を確認**

```bash
cargo build 2>&1 | tail -5
TMPIN=$(mktemp -d); TMPOUT=$(mktemp -d)
cp "input-samples2/2026-08-04 09.15.43.jpg" "input-samples2/2026-08-04 12.03.08.jpg" "$TMPIN/"
./target/debug/slidewarp "$TMPIN" -o "$TMPOUT" --on-low-confidence copy --report >/dev/null
grep -c "data:image/jpeg;base64," "$TMPOUT/report.html"
```

Expected: ビルド成功。`grep -c` が **4以上**（2枚×元/処理後）。`report.html` に `data:image/jpeg;base64,` が埋め込まれていること。

- [ ] **Step 6: コミット**

```bash
git add src/report.rs src/main.rs
git commit -m "feat(report): 表示をサムネdata URI化・拡大をURLエンコードして表示欠落を解消"
```

---

### Task 4: 信頼ベースラインの確立（手動評価）

**Files:** なし（運用タスク）。**分割理由**: レビュアーは「表示欠落ゼロ」を独立に検収でき、以降の検出改善の測定基準になる。

- [ ] **Step 1: release ビルドで402枚のレポート生成**

```bash
cargo build --release 2>&1 | tail -3
rm -rf eval-output && ./target/release/slidewarp input-samples2 -o eval-output/ --on-low-confidence copy --report
grep -c "data:image/jpeg;base64," eval-output/report.html
```

Expected: `grep -c` が概ね **枚数×2 に近い値**（検出不能で処理後なしの数枚を除く）。

- [ ] **Step 2: 表示欠落ゼロを確認**

`eval-output/report.html` をブラウザで開き、**全カードで元画像・処理後が表示される**ことを目視確認（旧「画像が出ていない」だった 09.15.43 / 12.12.40 / 11.32.14 / 12.18.42 が表示されること）。

- [ ] **Step 3: 全数を人手評価してベースライン JSON を保存**

report 上で crop/look を全数評価 → エクスポートした `slidewarp-eval.json` を
`docs/superpowers/baselines/2026-08-08-input-samples2-baseline.json` として保存。
旧24枚（`input-samples`）も同様に別途生成・評価し `..-input-samples-baseline.json` として保存。

- [ ] **Step 4: ベースラインをコミット**

```bash
mkdir -p docs/superpowers/baselines
git add docs/superpowers/baselines/
git commit -m "docs(eval): 信頼ベースライン(402枚+旧24枚)を記録"
```

---

## Pillar ②: ダークテーマ sub-slide 対策（A案）

### Task 5: 投影領域マスクと最大連結成分 bbox

**Files:**
- Modify: `src/detect.rs`（`brightness_mask` 近傍に関数追加＋`#[cfg(test)]`）

**Interfaces:**
- Produces:
  - `pub fn screen_mask(gray: &GrayImage) -> GrayImage` — 準黒(暗幕/室内)より上を前景化し、強めの close で投影面を1塊にした低しきい値マスク。
  - `fn largest_component_bbox(mask: &GrayImage) -> Option<(u32, u32, u32, u32)>` — 前景の最大連結成分の bbox `(x0,y0,x1,y1)`。

- [ ] **Step 1: 失敗するテストを書く**

`src/detect.rs` 末尾に `#[cfg(test)]` モジュール（無ければ新規）を追加:

```rust
#[cfg(test)]
mod dark_tests {
    use super::*;
    use image::{GrayImage, Luma};

    // 準黒背景(値8)の中央に暗グレー(値55)の矩形 = 投影面を模す
    fn synth_dark() -> GrayImage {
        let mut g = GrayImage::from_pixel(400, 300, Luma([8]));
        for y in 60..240 { for x in 80..320 { g.put_pixel(x, y, Luma([55])); } }
        // 内部の明るい要素(値230)
        for y in 100..160 { for x in 120..200 { g.put_pixel(x, y, Luma([230])); } }
        g
    }

    #[test]
    fn screen_mask_captures_whole_projection() {
        let g = synth_dark();
        let m = screen_mask(&g);
        let bb = largest_component_bbox(&m).expect("bbox");
        // 投影面(80..320, 60..240)にほぼ一致（close の膨張で数px外れは許容）
        assert!(bb.0 <= 85 && bb.1 <= 65 && bb.2 >= 315 && bb.3 >= 235,
                "bbox too small: {:?}", bb);
    }

    #[test]
    fn largest_component_bbox_none_on_empty() {
        let m = GrayImage::from_pixel(10, 10, Luma([0]));
        assert!(largest_component_bbox(&m).is_none());
    }
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --bin slidewarp dark_tests 2>&1 | tail -20`
Expected: `cannot find function screen_mask` で FAIL。

- [ ] **Step 3: 最小実装を書く**

`brightness_mask`（74-86行）の直後に追加:

```rust
/// プロジェクタ投影領域マスク（テーマ非依存）。暗幕/室内の「ほぼ黒」に対し、
/// ダークテーマでも真っ黒にならず暗グレーで浮く投影面を、低しきい値で前景化する。
/// 強めの close で内部の暗い図版ごと投影面を1塊にする（bbox 取得が目的）。
pub fn screen_mask(gray: &GrayImage) -> GrayImage {
    let blur = gaussian_blur_f32(gray, 2.0);
    // 準黒(暗幕)と暗グレー(投影面)を分ける低しきい値。大津の下側寄り、下限28/上限90でクランプ。
    let otsu = otsu_level(&blur) as f64;
    let thr = ((otsu * 0.5) as u32).clamp(28, 90) as u8;
    let mut mask = GrayImage::new(gray.width(), gray.height());
    for (m, b) in mask.pixels_mut().zip(blur.pixels()) {
        m[0] = if b[0] >= thr { 255 } else { 0 };
    }
    // 投影面内部の暗い切れ目を埋めて1塊化（半径6の close×2）。
    let mask = close(&mask, Norm::LInf, 6);
    let mask = close(&mask, Norm::LInf, 6);
    open(&mask, Norm::LInf, 3)
}

/// 前景(>0)の最大連結成分の bbox。連結成分が無ければ None。
fn largest_component_bbox(mask: &GrayImage) -> Option<(u32, u32, u32, u32)> {
    let (w, h) = mask.dimensions();
    let labels = imageproc::region_labelling::connected_components(
        mask,
        imageproc::region_labelling::Connectivity::Four,
        Luma([0u8]),
    );
    // ラベルごとの面積と bbox を集計
    let mut area: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut bb: std::collections::HashMap<u32, (u32, u32, u32, u32)> = std::collections::HashMap::new();
    for y in 0..h {
        for x in 0..w {
            let l = labels.get_pixel(x, y)[0];
            if l == 0 {
                continue;
            }
            *area.entry(l).or_insert(0) += 1;
            let e = bb.entry(l).or_insert((x, y, x, y));
            e.0 = e.0.min(x);
            e.1 = e.1.min(y);
            e.2 = e.2.max(x);
            e.3 = e.3.max(y);
        }
    }
    let best = area.iter().max_by_key(|(_, &a)| a)?.0;
    bb.get(best).copied()
}
```

（`connected_components` の返すラベル画像は `Luma<u32>`。既存 `use` に無ければフルパスで参照しているため追加 use 不要。`std::collections::HashMap` はフルパスで使用。）

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --bin slidewarp dark_tests 2>&1 | tail -20`
Expected: `test result: ok. 2 passed`。

- [ ] **Step 5: コミット**

```bash
git add src/detect.rs
git commit -m "feat(detect): 投影領域マスクscreen_maskと最大連結成分bboxを追加"
```

---

### Task 6: ダークテーマ判定

**Files:**
- Modify: `src/detect.rs`

**Interfaces:**
- Consumes: `bright_bbox`（既存）, `count_inside`（既存, `src/detect.rs:493`）。
- Produces: `fn is_dark_theme(gray: &GrayImage, bright: &GrayImage, screen_bbox: (u32,u32,u32,u32)) -> bool` — 投影面が十分大きく、かつ投影面内で「明部(brightness_mask)」が占める割合が小さいときに真。

- [ ] **Step 1: 失敗するテストを書く**

`dark_tests` モジュールに追記:

```rust
#[test]
fn dark_theme_true_for_dark_slide() {
    let g = synth_dark();
    let bright = brightness_mask(&g);
    let sm = screen_mask(&g);
    let bb = largest_component_bbox(&sm).unwrap();
    assert!(is_dark_theme(&g, &bright, bb), "dark slide should be dark theme");
}

#[test]
fn dark_theme_false_for_bright_slide() {
    // 全面が明るい(値210)スライド = 明るいテーマ
    let mut g = image::GrayImage::from_pixel(400, 300, image::Luma([12]));
    for y in 50..250 { for x in 70..330 { g.put_pixel(x, y, image::Luma([210])); } }
    let bright = brightness_mask(&g);
    let sm = screen_mask(&g);
    let bb = largest_component_bbox(&sm).unwrap();
    assert!(!is_dark_theme(&g, &bright, bb), "bright slide must not be dark theme");
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `cargo test --bin slidewarp dark_tests 2>&1 | tail -20`
Expected: `cannot find function is_dark_theme` で FAIL。

- [ ] **Step 3: 最小実装を書く**

`largest_component_bbox` の直後に追加:

```rust
/// ダークテーマ判定: 投影面が画像の一定割合以上を占め、かつ投影面内で
/// 明部(brightness_mask)が占める割合が小さい（＝スライド全体が暗い）とき真。
fn is_dark_theme(gray: &GrayImage, bright: &GrayImage, screen_bbox: (u32, u32, u32, u32)) -> bool {
    let (w, h) = gray.dimensions();
    let (x0, y0, x1, y1) = screen_bbox;
    let bw = (x1 - x0 + 1) as f64;
    let bh = (y1 - y0 + 1) as f64;
    let screen_ratio = (bw * bh) / ((w * h) as f64);
    if screen_ratio < 0.10 {
        return false; // 投影面が小さすぎる（遠景等）は対象外
    }
    let mut bright_in = 0u32;
    let mut total = 0u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            total += 1;
            if bright.get_pixel(x, y)[0] > 0 {
                bright_in += 1;
            }
        }
    }
    let bright_frac = if total > 0 { (bright_in as f64) / (total as f64) } else { 0.0 };
    // 投影面の半分未満しか「明部」でない＝ダークテーマ
    bright_frac < 0.5
}
```

- [ ] **Step 4: テストが通ることを確認**

Run: `cargo test --bin slidewarp dark_tests 2>&1 | tail -20`
Expected: `test result: ok. 4 passed`（Task 5 の2件＋本2件）。

- [ ] **Step 5: コミット**

```bash
git add src/detect.rs
git commit -m "feat(detect): ダークテーマ判定is_dark_themeを追加"
```

---

### Task 7: デュアル候補生成（投影領域からも生成）

**Files:**
- Modify: `src/detect.rs:303`（`hough_candidates` に bbox 明示引数を追加）と `src/detect.rs:906-951`（`detect_slide` の候補生成）

**Interfaces:**
- Consumes: `screen_mask`, `largest_component_bbox`（Task 5）, `contour_candidates`（既存）。
- Produces: `hough_candidates(gray, mask, ignore, bbox_override: Option<(u32,u32,u32,u32)>) -> Vec<geo::Quad>`（引数追加）。`detect_slide` が投影領域由来の候補を候補プールへマージする。

- [ ] **Step 1: `hough_candidates` に bbox 明示引数を追加**

`src/detect.rs:303` のシグネチャと bbox 決定部（303-308行）を次に置換:

```rust
fn hough_candidates(
    gray: &GrayImage,
    mask: &GrayImage,
    ignore: Option<&GrayImage>,
    bbox_override: Option<(u32, u32, u32, u32)>,
) -> Vec<geo::Quad> {
    let (w, h) = gray.dimensions();
    let (bx, by, bw, bh) = match bbox_override.or_else(|| bright_bbox(mask)) {
        Some((x0, y0, x1, y1)) => (x0, y0, x1 - x0 + 1, y1 - y0 + 1),
        None => return Vec::new(),
    };
```

- [ ] **Step 2: 既存の `hough_candidates` 呼び出しに `None` を追加**

`src/detect.rs` 内の既存2箇所（`detect_slide` の 928行・948行付近）を修正:

```rust
    for q in hough_candidates(&gray, &mask, None, None) {
        raw.push((q, "hough"));
    }
```

および ignore ありの呼び出し:

```rust
        for q in hough_candidates(&gray, &mask, Some(ig), None) {
            raw.push((q, "hough"));
        }
```

- [ ] **Step 3: ビルドで既存挙動が壊れていないこと（引数追加のみ）を確認**

Run: `cargo build 2>&1 | tail -5`
Expected: 成功（この時点では挙動不変）。

- [ ] **Step 4: `detect_slide` に投影領域マスクとデュアル生成を追加**

`src/detect.rs:909-910`（`let mask = ...; let mask_filled = ...;` の直後）に追加:

```rust
    let s_mask = screen_mask(&gray);
    let screen_bbox = largest_component_bbox(&s_mask);
    let dark = screen_bbox.map(|bb| is_dark_theme(&gray, &mask, bb)).unwrap_or(false);
```

`src/detect.rs:931`（`if let Some(q) = min_area_rect_of(&mask)` の直前）に、ダーク時のデュアル生成を追加:

```rust
    // ダークテーマ時: 投影領域マスク/ bbox からも全スライド候補を生成してマージ
    if dark {
        if let Some(bb) = screen_bbox {
            for q in hough_candidates(&gray, &mask, None, Some(bb)) {
                raw.push((q, "hough"));
            }
        }
        for q in contour_candidates(&gray, &s_mask, &edges_base) {
            raw.push((q, "contour"));
        }
    }
```

- [ ] **Step 5: ビルドとダーク例での候補改善を確認**

```bash
cargo build 2>&1 | tail -5
TMPIN=$(mktemp -d); TMPOUT=$(mktemp -d)
cp "input-samples2/2026-08-04 12.03.08.jpg" "input-samples2/2026-08-04 11.46.51.jpg" "$TMPIN/"
./target/debug/slidewarp "$TMPIN" -o "$TMPOUT" --on-low-confidence copy >/dev/null
```

Expected: ビルド成功・実行成功。（この時点では採点未変更のため選択は変わらない可能性大。次タスクで採点を直す。出力画像を目視し、少なくともクラッシュ無く候補が増えていること。）

- [ ] **Step 6: コミット**

```bash
git add src/detect.rs
git commit -m "feat(detect): ダーク時に投影領域からのデュアル候補生成を追加"
```

---

### Task 8: theme-aware スコアリング（fill/cut のマスク切替）

**Files:**
- Modify: `src/detect.rs:572`（`score_quad` シグネチャと fill/edge 呼び出し）と `src/detect.rs:955-956`（`detect_slide` の呼び出し）

**Interfaces:**
- Consumes: `screen_mask`（Task 5）, `is_dark_theme` の結果 `dark`（Task 6）, `edge_profile`（既存）, `count_inside`（既存）。
- Produces: `score_quad(quad, w, h, gray, mask_filled, edges_dil, gray_blur, dark, screen_mask) -> Option<(f64, Parts)>`。ダーク時は fill と cut の判定に `screen_mask` を用いる。

- [ ] **Step 1: `score_quad` に dark/screen_mask を渡し、fill/edge のマスクを切替**

`src/detect.rs:572-580` のシグネチャを次に置換（末尾に2引数追加）:

```rust
pub fn score_quad(
    quad: &geo::Quad,
    w: u32,
    h: u32,
    gray: &GrayImage,
    mask_filled: &GrayImage,
    edges_dil: &GrayImage,
    gray_blur: &GrayImage,
    dark: bool,
    screen_mask: &GrayImage,
) -> Option<(f64, Parts)> {
```

`src/detect.rs:599-605`（fill と edge_profile の算出）を次に置換:

```rust
    // ダーク時は「明部mask」でなく「投影領域mask」で fill/cut を評価する。
    // これにより暗く一様なスライド内部が不当に低fillにならず（逆転解消）、
    // かつ内部小矩形の両側が投影面内＝cut が発火して sub-slide を減点できる。
    let fill_mask: &GrayImage = if dark { screen_mask } else { mask_filled };
    let (inside_area, bright_inside, _, _) = count_inside(quad, w, h, Some(fill_mask));
    let fill = if inside_area > 0.0 {
        bright_inside / inside_area
    } else {
        0.0
    };
    let (edge, cut) = edge_profile(quad, edges_dil, fill_mask, gray_blur);
    let cut_score = 1.0 - (1.5 * cut).min(1.0);
```

（`edge_profile` の第3引数 `bright` に `fill_mask` を渡すのが cut 緩和の実体。明るいスライドでは `fill_mask == mask_filled` なので**挙動は完全に不変**。）

- [ ] **Step 2: `detect_slide` の呼び出しに dark/s_mask を渡す**

`src/detect.rs:956` の `score_quad` 呼び出しを次に置換:

```rust
        if let Some((mut score, parts)) = score_quad(&q, w, h, &gray, &mask_filled, &edges_dil, &gray_blur, dark, &s_mask) {
```

- [ ] **Step 3: ビルドとユニットテスト**

Run: `cargo build 2>&1 | tail -5 && cargo test --bin slidewarp 2>&1 | tail -15`
Expected: ビルド成功、既存ユニットテスト（report/dark_tests）全通過。

- [ ] **Step 4: ダーク sub-slide 群での改善を目視確認**

```bash
TMPIN=$(mktemp -d); TMPOUT=$(mktemp -d)
for f in "12.03.08" "12.03.39" "12.03.58" "12.05.41" "11.40.55" "11.46.51"; do cp "input-samples2/2026-08-04 $f.jpg" "$TMPIN/"; done
./target/debug/slidewarp "$TMPIN" -o "$TMPOUT" --on-low-confidence copy
```

Expected: 上記の出力画像で、内部要素だけでなく**スライド全体**が切り出されるようになっていること（`$TMPOUT` の画像を Read で目視）。

- [ ] **Step 5: コミット**

```bash
git add src/detect.rs
git commit -m "feat(detect): ダーク時のfill/cutを投影領域maskで評価するtheme-awareスコアリング"
```

---

### Task 9: 全数フレッシュ再評価と回帰ゼロ確認（採否判断）

**Files:** なし（運用タスク）。**分割理由**: CLAUDE.md の「全数・回帰ゼロ確認後に採用」を独立ゲートとする。

- [ ] **Step 1: release ビルドで eval-output をフレッシュ再生成（新データ）**

```bash
cargo build --release 2>&1 | tail -3
rm -rf eval-output && ./target/release/slidewarp input-samples2 -o eval-output/ --on-low-confidence copy --report
```

- [ ] **Step 2: 旧24枚でも再生成して回帰確認**

```bash
rm -rf eval-output-old && ./target/release/slidewarp input-samples -o eval-output-old/ --on-low-confidence copy --report
```

`eval-output-old/report.html` を Task 4 の旧24枚ベースラインと目視比較し、**悪化ゼロ**を確認（特に青被り 08.45.45・超斜め 19.55.25 など既知境界例）。

- [ ] **Step 3: 新データの改善と非悪化を確認**

`eval-output/report.html` を Task 4 の 402枚ベースラインと比較し、ダーク sub-slide 群が改善し、**従来良好だったスライド（明るいスライド・旧「表示欠落」の実良好例）が悪化していない**ことを確認。悪化例があれば `is_dark_theme` の閾値（`screen_ratio<0.10`, `bright_frac<0.5`）と `screen_mask` の `thr`（`otsu*0.5`, clamp 28..90）を調整し Step 1 から再実行。

- [ ] **Step 4: 結果を記録してコミット**

改善数・回帰数・調整したパラメータを設計ドキュメント末尾か
`docs/superpowers/baselines/2026-08-08-result.md` に追記してコミット:

```bash
git add docs/superpowers/
git commit -m "docs(eval): ダーク対策の全数再評価結果(改善/回帰ゼロ)を記録"
```

---

## Self-Review 結果（計画→仕様の突合）

- Pillar ①（表示欠落解消・自己完結HTML・ベースライン）→ Task 1–4 で網羅。
- Pillar ②（投影領域マスク／デュアル生成／theme-aware fill・cut／条件発火・回帰ゼロ）→ Task 5–9 で網羅。
- スコープ外（上端クリップ・天井混入・左右クリップ・歪み・2段ズーム・Python移植）は本計画に含めない（仕様通り）。
- 型整合: `screen_mask`/`largest_component_bbox`/`is_dark_theme`/`hough_candidates(...,None)`/`score_quad(...,dark,&s_mask)` の名称・引数は各タスク間で一致。
- プレースホルダ: 検出しきい値は具体値を明記し、Task 9 の評価ステップで調整する運用（設計上の反復であり TODO ではない）。
