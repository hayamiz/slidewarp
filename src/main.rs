//! slidewarp（純Rust版）CLI エントリポイント。
//! 学会撮影スライド写真を一括で検出・台形補正・シャープ化する。

mod detect;
mod enhance;
mod geometry;
mod report;
mod warp;

use std::path::{Path, PathBuf};

use clap::Parser;
use image::RgbImage;
use rayon::prelude::*;
use walkdir::WalkDir;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "bmp", "tif", "tiff", "webp"];

/// EXIF の回転情報を適用して RGB 画像を読み込む。`image` crate は自動適用しないため、
/// スマホ縦持ち撮影（Orientation=6/8 等）で横倒しにならないよう明示的に補正する
/// （Python 版の cv2.imdecode は EXIF 回転を適用するため、それに揃える）。
fn load_oriented(path: &Path) -> anyhow::Result<RgbImage> {
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let orientation = image::ImageDecoder::orientation(&mut decoder)
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img.to_rgb8())
}

#[derive(Parser)]
#[command(name = "slidewarp", about = "学会撮影スライド写真を検出・台形補正・シャープ化する（純Rust版）")]
struct Args {
    /// 写真ファイル または フォルダ（再帰探索）
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    /// 出力ディレクトリ
    #[arg(short, long)]
    out_dir: PathBuf,
    /// 露出/コントラストの自動補正
    #[arg(long)]
    exposure: bool,
    /// ホワイトバランス補正（gray-world）
    #[arg(long)]
    color: bool,
    /// シャープ化の強さ（0で無効）
    #[arg(long, default_value_t = 1.0)]
    sharpen: f32,
    /// この信頼度未満は誤補正回避のため出力しない
    #[arg(long, default_value_t = 0.35)]
    min_confidence: f64,
    /// 低信頼時の挙動: skip / copy
    #[arg(long, default_value = "skip")]
    on_low_confidence: String,
    /// 出力画像の長辺(px)上限
    #[arg(long, default_value_t = 1600)]
    max_long_side: u32,
    /// 検出矩形を各辺この比率だけ外へ広げて切り出す（周辺マージン。0で無効）
    #[arg(long, default_value_t = 0.03)]
    margin: f64,
    /// 出力拡張子（.jpg/.png）
    #[arg(long, default_value = ".jpg")]
    ext: String,
    /// JPEG品質(1-100)
    #[arg(long, default_value_t = 95)]
    jpeg_quality: u8,
    /// 並列数（0で自動）
    #[arg(short = 'j', long, default_value_t = 0)]
    jobs: usize,
    /// 評価用 report.html を生成しない
    #[arg(long)]
    no_report: bool,
    /// デバッグ: 各画像の幾何（見かけ/復元アスペクト・persp・決定比）を出力して終了
    #[arg(long)]
    dump_geom: bool,
}

struct ProcResult {
    src: PathBuf,
    status: &'static str, // ok / low_confidence / no_detection / error
    out_path: Option<PathBuf>,
    confidence: f64,
    method: String,
    message: String,
    parts: Option<detect::Parts>,
}

fn collect_inputs(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let is_img = |p: &Path| {
        p.extension()
            .and_then(|e| e.to_str())
            .map(|e| IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false)
    };
    for inp in inputs {
        if inp.is_dir() {
            let mut files: Vec<PathBuf> = WalkDir::new(inp)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file() && is_img(e.path()))
                .map(|e| e.into_path())
                .collect();
            files.sort();
            for f in files {
                if seen.insert(f.clone()) {
                    out.push(f);
                }
            }
        } else if inp.is_file() && is_img(inp) {
            if seen.insert(inp.clone()) {
                out.push(inp.clone());
            }
        }
    }
    out
}

fn output_path(src: &Path, out_dir: &Path, suffix: &str, ext: &str) -> PathBuf {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let mut cand = out_dir.join(format!("{stem}{suffix}{ext}"));
    let mut i = 1;
    while cand.exists() {
        cand = out_dir.join(format!("{stem}{suffix}_{i}{ext}"));
        i += 1;
    }
    cand
}

fn save_image(img: &RgbImage, path: &Path, quality: u8) -> anyhow::Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("jpg").to_lowercase();
    if ext == "jpg" || ext == "jpeg" {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, quality);
        enc.encode_image(img)?;
    } else {
        img.save(path)?;
    }
    Ok(())
}

fn process_image(src: &Path, args: &Args) -> ProcResult {
    let mk_err = |msg: String| ProcResult {
        src: src.to_path_buf(),
        status: "error",
        out_path: None,
        confidence: 0.0,
        method: String::new(),
        message: msg,
        parts: None,
    };
    let img = match load_oriented(src) {
        Ok(i) => i,
        Err(e) => return mk_err(format!("読み込み失敗: {e}")),
    };
    let det = detect::detect_slide(&img, None);
    let ext = if args.ext.starts_with('.') { args.ext.clone() } else { format!(".{}", args.ext) };

    let reject = |status: &'static str, conf: f64, method: String| -> ProcResult {
        if args.on_low_confidence == "copy" {
            let review = args.out_dir.join("_review");
            let _ = std::fs::create_dir_all(&review);
            let sext = src.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
            let op = output_path(src, &review, "_orig", &sext);
            let _ = std::fs::copy(src, &op);
            ProcResult { src: src.to_path_buf(), status, out_path: Some(op), confidence: conf, method,
                message: "低信頼のため原本を _review へ退避".into(), parts: None }
        } else {
            ProcResult { src: src.to_path_buf(), status, out_path: None, confidence: conf, method,
                message: "低信頼のためスキップ".into(), parts: None }
        }
    };

    let quad = match det.quad {
        None => return reject("no_detection", 0.0, det.method),
        Some(q) => q,
    };
    if det.confidence < args.min_confidence {
        return reject("low_confidence", det.confidence, det.method);
    }
    let warped = warp::warp_to_rect(&img, &quad, args.max_long_side, args.margin);
    let result = enhance::enhance(&warped, args.sharpen, args.exposure, args.color);
    if let Err(e) = std::fs::create_dir_all(&args.out_dir) {
        return mk_err(format!("出力先作成失敗: {e}"));
    }
    let op = output_path(src, &args.out_dir, "", &ext);
    if let Err(e) = save_image(&result, &op, args.jpeg_quality) {
        return mk_err(format!("書き出し失敗: {e}"));
    }
    ProcResult {
        src: src.to_path_buf(),
        status: "ok",
        out_path: Some(op),
        confidence: det.confidence,
        method: det.method,
        message: String::new(),
        parts: det.parts,
    }
}

fn rel_path(from_dir: &Path, to_file: &Path) -> String {
    let base = from_dir.canonicalize().unwrap_or_else(|_| from_dir.to_path_buf());
    let target = to_file.canonicalize().unwrap_or_else(|_| to_file.to_path_buf());
    let bc: Vec<_> = base.components().collect();
    let tc: Vec<_> = target.components().collect();
    let mut i = 0;
    while i < bc.len() && i < tc.len() && bc[i] == tc[i] {
        i += 1;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in i..bc.len() {
        parts.push("..".into());
    }
    for c in &tc[i..] {
        parts.push(c.as_os_str().to_string_lossy().to_string());
    }
    parts.join("/")
}

fn log(res: &ProcResult) {
    let tag = match res.status {
        "ok" => "OK  ",
        "low_confidence" => "LOW ",
        "no_detection" => "NONE",
        _ => "ERR ",
    };
    let name = res.src.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let extra = res.out_path.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str())
        .map(|n| format!(" -> {n}")).unwrap_or_default();
    let msg = if res.message.is_empty() { String::new() } else { format!(" ({})", res.message) };
    println!("[{tag}] {name}  conf={:.2} {}{extra}{msg}", res.confidence, res.method);
}

fn main() {
    let args = Args::parse();
    if args.jobs > 0 {
        let _ = rayon::ThreadPoolBuilder::new().num_threads(args.jobs).build_global();
    }
    let files = collect_inputs(&args.inputs);
    if files.is_empty() {
        eprintln!("入力に処理可能な画像が見つかりませんでした。");
        std::process::exit(2);
    }
    if args.dump_geom {
        for f in &files {
            let img = match load_oriented(f) {
                Ok(i) => i,
                Err(_) => continue,
            };
            let det = detect::detect_slide(&img, None);
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            if let Some(q) = det.quad {
                let est = geometry::estimate_aspect(&q);
                let (rec, info) = geometry::rectified_aspect(&q, img.dimensions());
                let dec = geometry::decide_output_aspect(&q, img.dimensions());
                let lab = if (dec - 16.0 / 9.0).abs() < 0.01 { "16:9" } else { "4:3" };
                let qs = q
                    .iter()
                    .map(|p| format!("({:.0},{:.0})", p[0], p[1]))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!(
                    "{:5} est={:.3} rec={:?} persp={:?} [{}] quad={}  {}",
                    det.method,
                    est,
                    rec.map(|v| (v * 1000.0).round() / 1000.0),
                    info.persp.map(|v| (v * 1000.0).round() / 1000.0),
                    lab,
                    qs,
                    name
                );
            } else {
                println!("none  {}", name);
            }
        }
        return;
    }

    let jobs = if args.jobs > 0 { args.jobs } else { rayon::current_num_threads() };
    println!("対象 {} 枚 / 並列 {}", files.len(), jobs);

    let results: Vec<ProcResult> = files.par_iter().map(|f| process_image(f, &args)).collect();
    for r in &results {
        log(r);
    }

    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for r in &results {
        *counts.entry(r.status).or_insert(0) += 1;
    }
    println!("---");
    println!("集計: {}", counts.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", "));

    if !args.no_report {
        let items: Vec<report::Item> = results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let parts = r
                    .parts
                    .as_ref()
                    .map(|p| serde_json::to_value(p).unwrap())
                    .unwrap_or(serde_json::Value::Null);
                report::Item {
                    id: i,
                    name: r.src.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string(),
                    src: rel_path(&args.out_dir, &r.src),
                    out: r.out_path.as_ref().map(|p| rel_path(&args.out_dir, p)),
                    status: r.status.to_string(),
                    confidence: (r.confidence * 1000.0).round() / 1000.0,
                    method: r.method.clone(),
                    message: r.message.clone(),
                    parts,
                }
            })
            .collect();
        match report::write_report(items, &args.out_dir) {
            Ok(p) => println!("レビュー: {}", p.display()),
            Err(e) => eprintln!("レポート生成失敗: {e}"),
        }
    }

    if counts.get("ok").copied().unwrap_or(0) == 0 {
        std::process::exit(1);
    }
}
